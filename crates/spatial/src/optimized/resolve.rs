//! Resolves source-derived state shared by optimized GeoParquet output topologies.

use anyhow::{Context, Result};
use arrow_schema::Schema;
use datafusion::dataframe::DataFrame;

use crate::geometry::Extent2D;
use crate::geoparquet::{NormalizedSpatialFrame, ResolvedReprojection, resolve_source};
use crate::input::{InputSource, RowRange, SourceDatasetMetadata};
use crate::optimized::extent_resolve::ExtentResolver;
use crate::optimized::multiscale::MultiscaleLevel;
use crate::output::MultiscaleEncoding;
use crate::pipeline::OutputExecutionOptions;

use super::multiscale::create_multiscale_levels;
use super::{ClusteringFamily, GeometryInfo};

/// Stores resolved source, geometry, projection, extent, and encoding state for optimized output.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedOptimization {
  source_metadata: SourceDatasetMetadata,
  geometry: GeometryInfo,
  reprojection: ResolvedReprojection,
  target_extent: Extent2D,
  levels: Vec<MultiscaleLevel>,
  multiscale_encoding: MultiscaleEncoding,
}

impl ResolvedOptimization {
  fn new(
    source_metadata: SourceDatasetMetadata,
    geometry: GeometryInfo,
    reprojection: ResolvedReprojection,
    target_extent: Extent2D,
    levels: Vec<MultiscaleLevel>,
    multiscale_encoding: MultiscaleEncoding,
  ) -> Self {
    Self {
      source_metadata,
      geometry,
      reprojection,
      target_extent,
      levels,
      multiscale_encoding,
    }
  }

  pub(crate) fn source_metadata(&self) -> &SourceDatasetMetadata {
    &self.source_metadata
  }

  pub(crate) fn geometry(&self) -> &GeometryInfo {
    &self.geometry
  }

  pub(crate) fn reprojection(&self) -> &ResolvedReprojection {
    &self.reprojection
  }

  pub(crate) fn target_extent(&self) -> Extent2D {
    self.target_extent
  }

  pub(crate) fn levels(&self) -> &[MultiscaleLevel] {
    &self.levels
  }

  pub(crate) fn multiscale_encoding(&self) -> MultiscaleEncoding {
    self.multiscale_encoding
  }

  pub(crate) fn delta_binary_packed_column_paths(&self) -> Vec<String> {
    self.multiscale_encoding.delta_binary_packed_column_paths(
      &self.levels,
      self.geometry.ty,
      self.geometry.has_z,
      self.geometry.has_m,
    )
  }
}

/// Resolve normalized data and immutable optimization facts for one output request.
pub(crate) async fn resolve_optimized_geoparquet(
  input: &dyn InputSource,
  input_dataframe: DataFrame,
  source_schema: &Schema,
  row_range: RowRange,
  options: &OutputExecutionOptions,
) -> Result<(DataFrame, ResolvedOptimization)> {
  let mut source = resolve_source(
    input,
    input_dataframe.clone(),
    source_schema,
    options.geometry_column.as_deref(),
    options.input_wkid,
    row_range,
  )
  .await?;
  source.strip_dimensions(options.strip_z, options.strip_m);
  let geometry = GeometryInfo::resolve(&source)?;
  let source_projjson = source
    .source_spatial_reference
    .projjson
    .as_ref()
    .context("missing resolved source CRS PROJJSON")?;
  let reprojection =
    ResolvedReprojection::from_source_projjson(source_projjson, options.output_wkid)?;
  let normalized = NormalizedSpatialFrame::new(
    input_dataframe,
    source_schema,
    &source,
    &reprojection,
    options.strip_z,
    options.strip_m,
  )?;
  let target_extent = ExtentResolver::new(input, row_range)
    .resolve(&source, &normalized, &reprojection)
    .await?;
  let levels = match geometry.clustering_family {
    ClusteringFamily::PointGeometry => Vec::new(),
    ClusteringFamily::ComplexGeometry => {
      create_multiscale_levels(options.output_wkid, geometry.ty)?
    }
  };
  let optimization = ResolvedOptimization::new(
    source.source_metadata,
    geometry,
    reprojection,
    target_extent,
    levels,
    options.multiscale_encoding,
  );
  Ok((normalized.dataframe(), optimization))
}
