use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};

#[allow(dead_code)]
pub(crate) fn transform_point_between_epsg(
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
