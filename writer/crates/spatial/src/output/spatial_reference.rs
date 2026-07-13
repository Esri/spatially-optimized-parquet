use serde_json::Value;

/// Selects the default output spatial reference.
pub const DEFAULT_OUTPUT_WKID: u32 = 4326;
/// Selects the projected spatial reference supported by display optimization.
pub const WEB_MERCATOR_OUTPUT_WKID: u32 = 3857;

#[derive(Debug, Clone, PartialEq, Default)]
/// Stores equivalent identifiers and definitions for one coordinate reference system.
pub struct SpatialReferenceInfo {
  /// Stores an EPSG well-known identifier when one can be inferred.
  pub wkid: Option<u32>,
  /// Stores a WKT definition when available.
  pub wkt: Option<String>,
  /// Stores the authoritative PROJJSON definition.
  pub projjson: Option<Value>,
}

/// Validate the requested output spatial reference before opening job resources.
pub(crate) fn validate_output_wkid(output_wkid: u32) {
  if output_wkid != DEFAULT_OUTPUT_WKID {
    todo!("output spatial reference EPSG:{output_wkid}");
  }
}
