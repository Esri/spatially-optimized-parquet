use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow_array::{BinaryArray, Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use gdal::vector::{Feature, Geometry, LayerAccess, LayerOptions};
use gdal::{Dataset, DriverManager};
use gdal_sys::{OGRFieldType, OGRwkbGeometryType};
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;
use spatial::input::gpkg::GpkgInputProvider;
use spatial::input::parquet::ParquetInputProvider;
use spatial::input::{InputOpenOptions, InputProvider, InputSource, open_input};
use tokio::runtime::Runtime;
use wkb::writer::write_geometry;

#[allow(dead_code)]
pub struct GpkgFeature<'a> {
  pub id: i32,
  pub name: Option<&'a str>,
  pub geometry_wkt: &'a str,
}

#[allow(dead_code)]
pub struct GpkgLayerSpec<'a> {
  pub name: &'a str,
  pub geometry_type: OGRwkbGeometryType::Type,
  pub epsg: Option<u32>,
  pub features: &'a [GpkgFeature<'a>],
}

#[allow(dead_code)]
pub fn sample_schema_with_geometry() -> SchemaRef {
  Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]))
}

#[allow(dead_code)]
pub fn sample_batch_with_geometry(wkb_values: Vec<Option<Vec<u8>>>) -> RecordBatch {
  let ids = Int32Array::from(vec![1, 2, 3]);
  let values: Vec<Option<&[u8]>> = wkb_values.iter().map(|value| value.as_deref()).collect();
  let geom = BinaryArray::from(values);
  RecordBatch::try_new(
    sample_schema_with_geometry(),
    vec![Arc::new(ids), Arc::new(geom)],
  )
  .unwrap()
}

#[allow(dead_code)]
pub fn write_parquet(
  path: &Path,
  schema: &SchemaRef,
  batches: &[RecordBatch],
  compression: Compression,
  kv: &[KeyValue],
) {
  let file = File::create(path).unwrap();
  let writer_properties = WriterProperties::builder()
    .set_compression(compression)
    .build();
  let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(writer_properties)).unwrap();
  for batch in batches {
    writer.write(batch).unwrap();
  }
  for kv in kv {
    writer.append_key_value_metadata(kv.clone());
  }
  writer.close().unwrap();
}

#[allow(dead_code)]
pub fn wkb_point(x: f64, y: f64) -> Vec<u8> {
  let geom = geo::Geometry::Point(geo::Point::new(x, y));
  let mut buf = Vec::new();
  write_geometry(&mut buf, &geom, &Default::default()).unwrap();
  buf
}

#[allow(dead_code)]
pub fn geoparquet_kv(primary_column: &str, geometry_types: &[&str]) -> KeyValue {
  geoparquet_kv_with_epsg(primary_column, geometry_types, 4326)
}

#[allow(dead_code)]
pub fn geoparquet_kv_with_epsg(
  primary_column: &str,
  geometry_types: &[&str],
  epsg: u32,
) -> KeyValue {
  let crs = SpatialRef::from_epsg(epsg).unwrap().to_projjson().unwrap();
  let crs: serde_json::Value = serde_json::from_str(&crs).unwrap();
  let types = geometry_types
    .iter()
    .map(|item| serde_json::Value::String((*item).to_string()))
    .collect::<Vec<_>>();
  let value = serde_json::json!({
      "version": "1.1.0",
      "primary_column": primary_column,
      "columns": {
          primary_column: {
              "encoding": "WKB",
              "geometry_types": types,
              "crs": crs
          }
      }
  });
  KeyValue::new("geo".to_string(), Some(value.to_string()))
}

#[allow(dead_code)]
pub fn write_gpkg(path: &Path, layers: &[GpkgLayerSpec<'_>]) {
  if path.exists() {
    std::fs::remove_file(path).unwrap();
  }

  let driver = DriverManager::get_driver_by_name("GPKG")
    .expect("GDAL GPKG driver is required for GeoPackage tests");
  let mut dataset = driver.create_vector_only(path).unwrap();

  for layer_spec in layers {
    let spatial_ref = layer_spec
      .epsg
      .map(|epsg| SpatialRef::from_epsg(epsg).unwrap());
    let geometry_options = ["GEOMETRY_NAME=geometry"];
    let layer = dataset
      .create_layer(LayerOptions {
        name: layer_spec.name,
        srs: spatial_ref.as_ref(),
        ty: layer_spec.geometry_type,
        options: Some(&geometry_options),
      })
      .unwrap();
    layer
      .create_defn_fields(&[
        ("id", OGRFieldType::OFTInteger),
        ("name", OGRFieldType::OFTString),
      ])
      .unwrap();
    let id_idx = layer.defn().field_index("id").unwrap();
    let name_idx = layer.defn().field_index("name").unwrap();

    for feature_spec in layer_spec.features {
      let mut feature = Feature::new(layer.defn()).unwrap();
      feature.set_field_integer(id_idx, feature_spec.id).unwrap();
      if let Some(name) = feature_spec.name {
        feature.set_field_string(name_idx, name).unwrap();
      }
      let geometry = Geometry::from_wkt(feature_spec.geometry_wkt).unwrap();
      feature.set_geometry(geometry).unwrap();
      feature.create(&layer).unwrap();
    }
  }

  dataset.flush_cache().unwrap();
}

#[allow(dead_code)]
pub fn open_parquet_input(path: &Path) -> Arc<dyn InputSource> {
  let providers: Vec<Box<dyn InputProvider>> = vec![Box::new(ParquetInputProvider::new())];
  runtime()
    .block_on(open_input(
      &InputOpenOptions::new(path.to_path_buf()),
      &providers,
    ))
    .unwrap()
}

#[allow(dead_code)]
pub fn open_gpkg_input(path: &Path, layer: Option<&str>) -> Arc<dyn InputSource> {
  let providers: Vec<Box<dyn InputProvider>> = vec![Box::new(GpkgInputProvider::new())];
  runtime()
    .block_on(open_input(
      &InputOpenOptions {
        location: path.to_string_lossy().into_owned(),
        layer: layer.map(str::to_string),
      },
      &providers,
    ))
    .unwrap()
}

#[allow(dead_code)]
pub fn open_gpkg_dataset(path: &Path) -> Dataset {
  Dataset::open(path).unwrap()
}

#[allow(dead_code)]
pub fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

#[allow(dead_code)]
pub fn transform_point_between_epsg(
  x: f64,
  y: f64,
  source_epsg: u32,
  target_epsg: u32,
) -> (f64, f64) {
  let mut source = SpatialRef::from_epsg(source_epsg).unwrap();
  source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
  let mut target = SpatialRef::from_epsg(target_epsg).unwrap();
  target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
  let transform = CoordTransform::new(&source, &target).unwrap();
  let mut xs = [x];
  let mut ys = [y];
  transform
    .transform_coords(&mut xs, &mut ys, &mut [])
    .unwrap();
  (xs[0], ys[0])
}
