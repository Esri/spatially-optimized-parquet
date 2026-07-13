//! Classifies input locations and enforces format-specific location constraints.

use std::path::Path;

use anyhow::Result;

use super::{InputOpenOptions, SourceFormat};

/// Return whether a location uses an HTTP or HTTPS scheme.
pub fn is_http_url(value: &str) -> bool {
  value.starts_with("http://") || value.starts_with("https://")
}

/// Reject a source format that cannot support the requested location shape.
pub(crate) fn require_local_path(
  format: SourceFormat,
  options: &InputOpenOptions,
) -> Result<&Path> {
  options
    .local_path()
    .ok_or_else(|| anyhow::anyhow!("{format} input does not support HTTP locations"))
}
