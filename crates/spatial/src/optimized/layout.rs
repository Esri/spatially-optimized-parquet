//! Resolves optimized GeoParquet layout decisions.

use anyhow::Result;

use crate::geoparquet::GeoParquetWriteContext;
use crate::optimized::MultiscaleEncoding;
use crate::optimized::multiscale::MultiscaleLevel;
use crate::pipeline::OutputOptions;

use super::{ClusteringFamily, GeometryInfo};

/// Defines clustering and encoding choices for optimized GeoParquet output.
#[derive(Debug, Clone)]
pub(crate) struct OptimizedLayout {
  geometry: GeometryInfo,
  levels: Vec<MultiscaleLevel>,
  multiscale_encoding: MultiscaleEncoding,
}

impl OptimizedLayout {
  /// Resolve optimized layout decisions from prepared GeoParquet data.
  pub(crate) fn new(
    context: &GeoParquetWriteContext,
    output_options: &OutputOptions,
  ) -> Result<Self> {
    let geometry = GeometryInfo::resolve(context.source())?;
    let levels = match geometry.clustering_family {
      ClusteringFamily::PointGeometry => Vec::new(),
      ClusteringFamily::ComplexGeometry => {
        MultiscaleLevel::create_all(output_options.output_wkid, geometry.ty)?
      }
    };
    Ok(Self {
      geometry,
      levels,
      multiscale_encoding: output_options.multiscale_encoding,
    })
  }

  /// Return geometry facts that select the clustering strategy.
  pub(crate) fn geometry(&self) -> &GeometryInfo {
    &self.geometry
  }

  /// Return multiscale levels for complex geometry output.
  pub(crate) fn levels(&self) -> &[MultiscaleLevel] {
    &self.levels
  }

  /// Return the selected multiscale payload encoding.
  pub(crate) fn multiscale_encoding(&self) -> MultiscaleEncoding {
    self.multiscale_encoding
  }

  /// Return payload columns that require delta-binary-packed Parquet encoding.
  pub(crate) fn delta_binary_packed_column_paths(&self) -> Vec<String> {
    self.multiscale_encoding.delta_binary_packed_column_paths(
      &self.levels,
      self.geometry.ty,
      self.geometry.has_z,
      self.geometry.has_m,
    )
  }
}
