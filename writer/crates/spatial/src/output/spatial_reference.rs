/// Selects the default output spatial reference.
pub const DEFAULT_OUTPUT_WKID: u32 = 4326;
/// Selects the projected spatial reference supported by display optimization.
pub const WEB_MERCATOR_OUTPUT_WKID: u32 = 3857;

/// Validate the requested output spatial reference before opening job resources.
pub(crate) fn validate_output_wkid(output_wkid: u32) {
  if output_wkid != DEFAULT_OUTPUT_WKID {
    todo!("output spatial reference EPSG:{output_wkid}");
  }
}
