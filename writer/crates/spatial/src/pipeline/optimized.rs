//! Resolves state shared by both optimized pipeline variants.

use anyhow::{Context, Result};

use crate::geoparquet::{resolve_source, validate_covering_configuration};
use crate::optimized::extent::TargetExtentResolver;
use crate::optimized::multiscale::create_geometry_encodings;
use crate::optimized::{ClusteringFamily, OptimizedGeometry, ResolvedOptimization};
use crate::output::reprojection::ReprojectionSpec;

use super::SpatialPipelineState;

pub(super) async fn resolve_optimization(
  state: &SpatialPipelineState,
) -> Result<ResolvedOptimization> {
  validate_covering_configuration(state.covering, state.source_schema.as_ref())?;
  let source = resolve_source(
    state.input.as_ref(),
    state.input_dataframe.clone(),
    state.source_schema.as_ref(),
    state.geometry_column.as_deref(),
    state.input_wkid,
    state.row_range,
  )
  .await?;
  let geometry = OptimizedGeometry::resolve(&source)?;
  let source_projjson = source
    .source_spatial_reference
    .projjson
    .as_ref()
    .context("missing resolved source CRS PROJJSON")?;
  let reprojection = ReprojectionSpec::from_source_projjson(source_projjson, state.output_wkid)?;
  let target_extent = TargetExtentResolver::new(
    state.input.as_ref(),
    state.input_dataframe.clone(),
    state.total_input_rows,
    state.row_range,
    state.progress,
    state.explain,
  )
  .resolve(&source, &geometry, &reprojection)
  .await?;
  let encodings = match geometry.clustering_family {
    ClusteringFamily::Point => Vec::new(),
    ClusteringFamily::NonPoint => {
      create_geometry_encodings(state.output_wkid, geometry.geometry_type)?
    }
  };
  Ok(ResolvedOptimization {
    source_metadata: source.source_metadata,
    geometry,
    reprojection,
    target_extent,
    encodings,
  })
}
