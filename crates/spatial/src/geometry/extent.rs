use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
/// Represents an axis-aligned two-dimensional extent.
pub(crate) struct Extent2D {
  /// Defines the minimum x coordinate.
  pub(crate) xmin: f64,
  /// Defines the minimum y coordinate.
  pub(crate) ymin: f64,
  /// Defines the maximum x coordinate.
  pub(crate) xmax: f64,
  /// Defines the maximum y coordinate.
  pub(crate) ymax: f64,
}
