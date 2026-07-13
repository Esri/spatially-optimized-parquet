use std::path::Path;

use gdal::spatial_ref::SpatialRef;
use gdal::vector::{Feature, Geometry, LayerAccess, LayerOptions};
use gdal::{Dataset, DriverManager};
use gdal_sys::{OGRFieldType, OGRwkbGeometryType};

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
pub fn open_gpkg_dataset(path: &Path) -> Dataset {
  Dataset::open(path).unwrap()
}
