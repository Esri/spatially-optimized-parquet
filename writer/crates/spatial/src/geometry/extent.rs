use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
/// Represents an axis-aligned two-dimensional extent.
pub(crate) struct Extent2D {
  /// Stores the minimum x coordinate.
  pub(crate) xmin: f64,
  /// Stores the minimum y coordinate.
  pub(crate) ymin: f64,
  /// Stores the maximum x coordinate.
  pub(crate) xmax: f64,
  /// Stores the maximum y coordinate.
  pub(crate) ymax: f64,
}
