use anyhow::Result;

use super::{OptimizeOutputRequest, prepare, write};

/// Run optimized planning, analysis, ordering, and durable output writing.
pub(crate) async fn run(request: OptimizeOutputRequest<'_>) -> Result<u64> {
  let planning = prepare::build_spatial_planning_context(&request).await?;
  let prepared = prepare::prepare_optimized_output(&request, &planning).await?;
  write::write_optimized_output(&request, prepared).await
}
