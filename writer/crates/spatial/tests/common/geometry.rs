use anyhow::{Context, Result, bail};
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use geo_traits::{
  CoordTrait, GeometryTrait, GeometryType, LineStringTrait, PointTrait, PolygonTrait,
};

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

pub fn point_xy_from_wkb(bytes: &[u8]) -> Result<(f64, f64)> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  match geometry.as_type() {
    GeometryType::Point(point) => point
      .coord()
      .map(|coord| coord.x_y())
      .context("point missing coordinate"),
    _ => bail!("expected point geometry"),
  }
}

pub fn polygon_extent_from_wkb(bytes: &[u8]) -> Result<[f64; 4]> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  let GeometryType::Polygon(polygon) = geometry.as_type() else {
    bail!("expected polygon geometry");
  };
  let exterior = polygon
    .exterior()
    .context("polygon missing exterior ring")?;
  let mut extent = [
    f64::INFINITY,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NEG_INFINITY,
  ];
  for coordinate in exterior.coords() {
    let (x, y) = coordinate.x_y();
    extent[0] = extent[0].min(x);
    extent[1] = extent[1].min(y);
    extent[2] = extent[2].max(x);
    extent[3] = extent[3].max(y);
  }
  Ok(extent)
}
