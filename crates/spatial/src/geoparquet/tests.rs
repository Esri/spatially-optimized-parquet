use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow_array::{BinaryArray, Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;
use futures_util::future::BoxFuture;
use gdal::spatial_ref::SpatialRef;
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;
use tempfile::TempDir;
use tokio::runtime::Runtime;

use crate::geometry::{Extent2D, GeometryColumn, GeometryEncoding, GeometryKind};
use crate::input::{
  InputError, InputOpenOptions, InputSource, RowRange, SourceDatasetMetadata, SourceFormat,
  SourceGeometryMetadata, open_input,
};

use crate::pipeline::{SpatialWriteContext, resolve_source};

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

fn sample_schema_with_geometry() -> SchemaRef {
  Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]))
}

fn sample_batch_with_geometry(wkb_values: Vec<Option<Vec<u8>>>) -> RecordBatch {
  let ids = Int32Array::from_iter_values(1..=wkb_values.len() as i32);
  let values = wkb_values
    .iter()
    .map(|value| value.as_deref())
    .collect::<Vec<_>>();
  RecordBatch::try_new(
    sample_schema_with_geometry(),
    vec![Arc::new(ids), Arc::new(BinaryArray::from(values))],
  )
  .unwrap()
}

fn wkb_point(x: f64, y: f64) -> Vec<u8> {
  crate::geometry::write_test_point(x, y)
}

fn write_parquet(
  path: &Path,
  schema: &SchemaRef,
  batches: &[RecordBatch],
  compression: Compression,
  metadata: &[KeyValue],
) {
  let properties = WriterProperties::builder()
    .set_compression(compression)
    .build();
  let mut writer = ArrowWriter::try_new(
    File::create(path).unwrap(),
    schema.clone(),
    Some(properties),
  )
  .unwrap();
  for batch in batches {
    writer.write(batch).unwrap();
  }
  for entry in metadata {
    writer.append_key_value_metadata(entry.clone());
  }
  writer.close().unwrap();
}

fn geoparquet_kv(primary_column: &str, geometry_types: &[&str]) -> KeyValue {
  geoparquet_kv_with_bbox(primary_column, geometry_types, None)
}

fn geoparquet_kv_with_bbox(
  primary_column: &str,
  geometry_types: &[&str],
  bbox: Option<[f64; 4]>,
) -> KeyValue {
  let spatial_reference = SpatialRef::from_epsg(4326).unwrap().to_projjson().unwrap();
  let spatial_reference: serde_json::Value = serde_json::from_str(&spatial_reference).unwrap();
  let geometry_types = geometry_types
    .iter()
    .map(|item| serde_json::Value::String((*item).to_string()))
    .collect::<Vec<_>>();
  let value = serde_json::json!({
    "version": "1.1.0",
    "primary_column": primary_column,
    "columns": {
      primary_column: {
        "encoding": "WKB",
        "geometry_types": geometry_types,
        "crs": spatial_reference,
        "bbox": bbox
      }
    }
  });
  KeyValue::new("geo".to_string(), Some(value.to_string()))
}

fn open_parquet_input(path: &Path) -> Arc<dyn InputSource> {
  runtime()
    .block_on(open_input(
      SourceFormat::Parquet,
      &InputOpenOptions::new(path.to_string_lossy().into_owned(), None),
    ))
    .unwrap()
}

struct MetadataInputSource {
  schema: SchemaRef,
  metadata: SourceDatasetMetadata,
  dataframe_calls: Arc<AtomicUsize>,
}

impl InputSource for MetadataInputSource {
  fn schema(&self) -> Result<SchemaRef, InputError> {
    Ok(self.schema.clone())
  }

  fn total_rows(&self) -> Result<u64, InputError> {
    Ok(3)
  }

  fn inferred_geometry_column(&self) -> Result<Option<GeometryColumn>, InputError> {
    Ok(Some(GeometryColumn {
      column: "geometry".into(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: Some(GeometryKind::Point),
    }))
  }

  fn source_metadata(&self) -> Result<SourceDatasetMetadata, InputError> {
    Ok(self.metadata.clone())
  }

  fn to_dataframe<'a>(
    &'a self,
    ctx: &'a SessionContext,
    _row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame, InputError>> {
    self.dataframe_calls.fetch_add(1, Ordering::SeqCst);
    Box::pin(async move {
      ctx
        .read_batch(RecordBatch::new_empty(self.schema.clone()))
        .map_err(|source| InputError::DataFusion {
          operation: "create metadata test dataframe",
          source,
        })
    })
  }
}

fn epsg_projjson(wkid: u32) -> serde_json::Value {
  serde_json::from_str(&SpatialRef::from_epsg(wkid).unwrap().to_projjson().unwrap()).unwrap()
}

fn source_dataframe(input: &dyn InputSource) -> DataFrame {
  let context = SessionContext::new();
  runtime()
    .block_on(input.to_dataframe(&context, RowRange::default()))
    .unwrap()
}

fn empty_dataframe(schema: SchemaRef) -> DataFrame {
  SessionContext::new()
    .read_batch(RecordBatch::new_empty(schema))
    .unwrap()
}

#[test]
fn inferred_geometry_column_reads_geoparquet_primary_column() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![Some(wkb_point(0.0, 0.0)), None, None]);
  let kv = vec![geoparquet_kv("geometry", &["Point"])];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let input = open_parquet_input(&path);
  let geometry_column = input.inferred_geometry_column().unwrap().unwrap();
  assert_eq!(geometry_column.column, "geometry");
  assert_eq!(geometry_column.geometry_kind, Some(GeometryKind::Point));
}

#[test]
fn source_metadata_collects_passthrough_kv_without_geo() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![None, None, None]);
  let kv = vec![
    geoparquet_kv("geometry", &["Point"]),
    KeyValue::new("source".to_string(), Some("census".to_string())),
  ];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let input = open_parquet_input(&path);
  let metadata = input.source_metadata().unwrap();
  let keys: HashMap<String, String> = metadata
    .passthrough_kv
    .into_iter()
    .filter_map(|kv| kv.value.map(|value| (kv.key, value)))
    .collect();
  assert_eq!(keys.get("source").map(String::as_str), Some("census"));
  assert!(!keys.contains_key("geo"));
}

#[test]
fn source_metadata_returns_none_when_geo_metadata_missing() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![Some(wkb_point(0.0, 0.0)), None, None]);
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[],
  );

  let input = open_parquet_input(&path);
  assert!(input.inferred_geometry_column().unwrap().is_none());
  assert!(input.source_metadata().unwrap().geometry.is_none());
}

#[test]
fn source_metadata_tolerates_null_bbox_metadata() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![Some(wkb_point(0.0, 0.0)), None, None]);
  let kv = vec![KeyValue::new(
        "geo".to_string(),
        Some(
            "{\"version\":\"1.1.0\",\"primary_column\":\"geometry\",\"columns\":{\"geometry\":{\"encoding\":\"WKB\",\"geometry_types\":[\"Point\"],\"bbox\":[null,null,null,null]}}}".to_string(),
        ),
    )];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let input = open_parquet_input(&path);
  let metadata = input.source_metadata().unwrap();
  let geometry = metadata.geometry.unwrap();
  assert_eq!(geometry.column, "geometry");
  assert!(geometry.bbox.is_none());
  assert_eq!(geometry.geometry_types, vec![GeometryKind::Point]);
}

#[test]
fn source_metadata_merges_multifile_geometry_types_and_extent() {
  let temp = TempDir::new().unwrap();
  let schema = sample_schema_with_geometry();
  for (name, geometry_type, bbox, point) in [
    (
      "first.parquet",
      "Point Z",
      [-10.0, -5.0, 1.0, 2.0],
      wkb_point(0.0, 0.0),
    ),
    (
      "second.parquet",
      "MultiPoint Z",
      [-20.0, 0.0, 30.0, 40.0],
      wkb_point(5.0, 5.0),
    ),
  ] {
    write_parquet(
      &temp.path().join(name),
      &schema,
      &[sample_batch_with_geometry(vec![Some(point)])],
      Compression::SNAPPY,
      &[geoparquet_kv_with_bbox(
        "geometry",
        &[geometry_type],
        Some(bbox),
      )],
    );
  }

  let metadata = open_parquet_input(temp.path()).source_metadata().unwrap();
  let geometry = metadata.geometry.unwrap();

  assert_eq!(
    geometry.geometry_types,
    vec![GeometryKind::MultiPoint, GeometryKind::Point]
  );
  assert_eq!(
    geometry.bbox,
    Some(Extent2D {
      xmin: -20.0,
      ymin: -5.0,
      xmax: 30.0,
      ymax: 40.0,
    })
  );
  assert!(geometry.has_z);
  assert!(!geometry.has_m);
}

#[test]
fn resolved_source_retains_spatial_reference_and_extent_in_source_metadata() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![
    Some(wkb_point(-10.0, 5.0)),
    Some(wkb_point(20.0, 30.0)),
    None,
  ]);
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  let input = open_parquet_input(&path);
  let context = runtime()
    .block_on(resolve_source(
      input.as_ref(),
      source_dataframe(input.as_ref()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
    ))
    .unwrap();
  let source_geometry = context.source_metadata.geometry.unwrap();

  assert_eq!(source_geometry.bbox, Some(context.source_extent));
  assert_eq!(
    source_geometry.projjson,
    context.source_spatial_reference.projjson
  );
}

#[test]
fn resolved_source_uses_complete_metadata_without_creating_dataframe() {
  let dataframe_calls = Arc::new(AtomicUsize::new(0));
  let expected_extent = Extent2D {
    xmin: -10.0,
    ymin: 5.0,
    xmax: 20.0,
    ymax: 30.0,
  };
  let input = MetadataInputSource {
    schema: sample_schema_with_geometry(),
    metadata: SourceDatasetMetadata {
      geometry: Some(SourceGeometryMetadata {
        column: "geometry".into(),
        encoding: GeometryEncoding::Wkb,
        geometry_types: vec![GeometryKind::Point],
        bbox: Some(expected_extent),
        covering: None,
        projjson: Some(epsg_projjson(4326)),
        has_z: false,
        has_m: false,
      }),
      passthrough_kv: Vec::new(),
    },
    dataframe_calls: dataframe_calls.clone(),
  };

  let context = runtime()
    .block_on(resolve_source(
      &input,
      empty_dataframe(input.schema().unwrap()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
    ))
    .unwrap();

  assert_eq!(context.geometry_types, vec![GeometryKind::Point]);
  assert_eq!(context.source_extent, expected_extent);
  assert_eq!(dataframe_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn write_context_reuses_complete_source_metadata_for_normalized_output() {
  let expected_extent = Extent2D {
    xmin: -10.0,
    ymin: 5.0,
    xmax: 20.0,
    ymax: 30.0,
  };
  let input = MetadataInputSource {
    schema: sample_schema_with_geometry(),
    metadata: SourceDatasetMetadata {
      geometry: Some(SourceGeometryMetadata {
        column: "geometry".into(),
        encoding: GeometryEncoding::Wkb,
        geometry_types: vec![GeometryKind::Point],
        bbox: Some(expected_extent),
        covering: None,
        projjson: Some(epsg_projjson(4326)),
        has_z: true,
        has_m: true,
      }),
      passthrough_kv: Vec::new(),
    },
    dataframe_calls: Arc::new(AtomicUsize::new(0)),
  };

  let context = runtime()
    .block_on(SpatialWriteContext::resolve(
      &input,
      empty_dataframe(input.schema().unwrap()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
      4326,
      true,
      false,
      None,
    ))
    .unwrap();

  assert_eq!(context.frame().geometry_column(), "geometry");
  assert_eq!(context.target_extent(), expected_extent);
  assert_eq!(
    context.reprojection().target_spatial_reference().wkid,
    Some(4326)
  );
  assert!(!context.source().has_z);
  assert!(context.source().has_m);
}

#[test]
fn resolved_source_prefers_top_level_spatial_reference_authority_code() {
  let mut projjson = epsg_projjson(4269);
  projjson["datum"]["id"] = serde_json::json!({
    "authority": "EPSG",
    "code": 6269
  });
  let input = MetadataInputSource {
    schema: sample_schema_with_geometry(),
    metadata: SourceDatasetMetadata {
      geometry: Some(SourceGeometryMetadata {
        column: "geometry".into(),
        encoding: GeometryEncoding::Wkb,
        geometry_types: vec![GeometryKind::Point],
        bbox: Some(Extent2D {
          xmin: -10.0,
          ymin: 5.0,
          xmax: 20.0,
          ymax: 30.0,
        }),
        covering: None,
        projjson: Some(projjson),
        has_z: false,
        has_m: false,
      }),
      passthrough_kv: Vec::new(),
    },
    dataframe_calls: Arc::new(AtomicUsize::new(0)),
  };

  let context = runtime()
    .block_on(resolve_source(
      &input,
      empty_dataframe(input.schema().unwrap()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
    ))
    .unwrap();

  assert_eq!(context.source_spatial_reference.wkid, Some(4269));
}

#[test]
fn resolved_source_preserves_projjson_for_unknown_spatial_reference_authority() {
  let mut projjson = epsg_projjson(4326);
  projjson["id"] = serde_json::json!({
    "authority": "IGNF",
    "code": 1234
  });
  let input = MetadataInputSource {
    schema: sample_schema_with_geometry(),
    metadata: SourceDatasetMetadata {
      geometry: Some(SourceGeometryMetadata {
        column: "geometry".into(),
        encoding: GeometryEncoding::Wkb,
        geometry_types: vec![GeometryKind::Point],
        bbox: Some(Extent2D {
          xmin: -10.0,
          ymin: 5.0,
          xmax: 20.0,
          ymax: 30.0,
        }),
        covering: None,
        projjson: Some(projjson.clone()),
        has_z: false,
        has_m: false,
      }),
      passthrough_kv: Vec::new(),
    },
    dataframe_calls: Arc::new(AtomicUsize::new(0)),
  };

  let context = runtime()
    .block_on(resolve_source(
      &input,
      empty_dataframe(input.schema().unwrap()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
    ))
    .unwrap();

  assert_eq!(context.source_spatial_reference.wkid, None);
  assert_eq!(context.source_spatial_reference.projjson, Some(projjson));
}
