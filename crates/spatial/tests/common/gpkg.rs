// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::path::Path;

use gdal::DriverManager;
use gdal::spatial_ref::SpatialRef;
use gdal::vector::{Feature, Geometry, LayerAccess, LayerOptions};
use gdal_sys::{OGRFieldType, OGRwkbGeometryType};

pub struct GpkgFeature<'a> {
  pub id: i32,
  pub name: Option<&'a str>,
  pub geometry_wkt: &'a str,
}

pub struct GpkgLayer<'a> {
  pub name: &'a str,
  pub geometry_type: OGRwkbGeometryType::Type,
  pub epsg: Option<u32>,
  pub features: &'a [GpkgFeature<'a>],
}

pub fn write_gpkg(path: &Path, layers: &[GpkgLayer<'_>]) {
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
    let id_index = layer.defn().field_index("id").unwrap();
    let name_index = layer.defn().field_index("name").unwrap();

    for feature_spec in layer_spec.features {
      let mut feature = Feature::new(layer.defn()).unwrap();
      feature
        .set_field_integer(id_index, feature_spec.id)
        .unwrap();
      if let Some(name) = feature_spec.name {
        feature.set_field_string(name_index, name).unwrap();
      }
      feature
        .set_geometry(Geometry::from_wkt(feature_spec.geometry_wkt).unwrap())
        .unwrap();
      feature.create(&layer).unwrap();
    }
  }

  dataset.flush_cache().unwrap();
}
