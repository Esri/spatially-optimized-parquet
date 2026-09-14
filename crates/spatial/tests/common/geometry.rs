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

use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};

use super::wkb;

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

pub fn point_from_wkb_xy(bytes: &[u8]) -> Result<(f64, f64), String> {
  wkb::read_point(bytes)
}

pub fn polygon_extent_from_wkb(bytes: &[u8]) -> Result<[f64; 4], String> {
  wkb::read_polygon_extent(bytes)
}
