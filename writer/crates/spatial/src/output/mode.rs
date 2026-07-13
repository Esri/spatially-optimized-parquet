/// Selects which GeoParquet product a job writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GeoParquetOutputMode {
  /// Writes Spatially Optimized GeoParquet.
  #[default]
  Optimized,
  /// Writes GeoParquet without SOP display columns or spatial sorting.
  Plain,
}
