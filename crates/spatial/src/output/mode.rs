/// Selects which GeoParquet product a job writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputMode {
  /// Writes Spatially Optimized GeoParquet.
  #[default]
  OptimizedGeoParquet,
  /// Writes GeoParquet without optimized clustering columns or spatial sorting.
  GeoParquet,
}
