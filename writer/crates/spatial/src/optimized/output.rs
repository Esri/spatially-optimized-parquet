use anyhow::Result;
use async_trait::async_trait;

use crate::geoparquet::{resolve_source_context, validate_covering_configuration};
use crate::optimized::clustering::{cluster_partition_column, validate_cluster_partition_column};
use crate::optimized::extent::resolve_target_extent;
use crate::optimized::metadata::build_optimized_metadata;
use crate::optimized::multiscale::create_geometry_encodings;
use crate::optimized::projection::build_optimized_projection;
use crate::optimized::{ClusteringFamily, OptimizedContext, OptimizedGeometry};
use crate::output::reprojection::ReprojectionContext;
use crate::output::stage::{OutputStage, OutputStageContext, OutputStageResult};

use super::write::write_optimized_output;

/// Provides Spatially Optimized GeoParquet through the shared output-stage boundary.
pub(crate) struct OptimizedGeoParquet;

#[async_trait]
impl OutputStage for OptimizedGeoParquet {
  async fn execute(&self, context: OutputStageContext<'_>) -> Result<OutputStageResult> {
    validate_covering_configuration(context.covering, context.source_schema)?;
    let source = resolve_source_context(
      context.input,
      context.source_schema,
      context.geometry_column,
      context.input_wkid,
      context.row_range,
    )
    .await?;
    let geometry = OptimizedGeometry::resolve(&source)?;
    let source_projjson = source
      .source_spatial_reference
      .projjson
      .as_ref()
      .ok_or_else(|| anyhow::anyhow!("missing resolved source CRS PROJJSON"))?;
    let reprojection =
      ReprojectionContext::from_source_projjson(source_projjson, context.output_wkid)?;
    let target_extent = resolve_target_extent(&context, &source, &geometry, &reprojection).await?;
    let encodings = match geometry.clustering_family {
      ClusteringFamily::Point => Vec::new(),
      ClusteringFamily::NonPoint => {
        create_geometry_encodings(context.output_wkid, geometry.geometry_type)?
      }
    };
    let optimized = OptimizedContext {
      source_metadata: source.source_metadata,
      geometry,
      reprojection,
      target_extent,
      encodings,
    };
    let partition_column = (context.output_layout.parts > 1).then_some(cluster_partition_column(
      optimized.geometry.clustering_family,
    ));
    validate_cluster_partition_column(context.source_schema, partition_column)?;
    let dataframe = build_optimized_projection(&context, &optimized).await?;
    let metadata = build_optimized_metadata(&optimized, context.covering)?;
    let rows_written = write_optimized_output(&context, dataframe, &optimized, metadata).await?;
    Ok(OutputStageResult { rows_written })
  }
}
