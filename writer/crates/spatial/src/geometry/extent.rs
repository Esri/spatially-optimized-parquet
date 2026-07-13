use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
/// Represents an axis-aligned two-dimensional extent.
pub struct Extent2D {
  /// Stores the minimum x coordinate.
  pub xmin: f64,
  /// Stores the minimum y coordinate.
  pub ymin: f64,
  /// Stores the maximum x coordinate.
  pub xmax: f64,
  /// Stores the maximum y coordinate.
  pub ymax: f64,
}
