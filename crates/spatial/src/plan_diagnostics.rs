//! Prints DataFusion physical plans for explicit diagnostic builds.

use anyhow::Result;
use arrow_array::RecordBatch;
use datafusion::dataframe::DataFrame;
use datafusion::physical_plan::ExecutionPlan;

/// Collect one DataFrame while printing its physical plan when diagnostics are enabled.
pub(crate) async fn collect_dataframe(
  dataframe: DataFrame,
  label: &str,
) -> Result<Vec<RecordBatch>> {
  #[cfg(feature = "print-plan")]
  {
    use std::sync::Arc;

    use datafusion::execution::TaskContext;
    use datafusion::physical_plan::collect;

    let (state, logical_plan) = dataframe.into_parts();
    let context = Arc::new(TaskContext::from(&state));
    let plan = state.create_physical_plan(&logical_plan).await?;
    print_physical_plan(label, plan.as_ref());
    Ok(collect(plan, context).await?)
  }

  #[cfg(not(feature = "print-plan"))]
  {
    let _ = label;
    Ok(dataframe.collect().await?)
  }
}

/// Print one physical plan when the `print-plan` feature is enabled.
pub(crate) fn print_physical_plan(label: &str, plan: &dyn ExecutionPlan) {
  #[cfg(feature = "print-plan")]
  {
    use datafusion::physical_plan::displayable;

    eprintln!(
      "\n=== DataFusion physical plan: {label} ===\n{}\n",
      displayable(plan).indent(true)
    );
  }

  #[cfg(not(feature = "print-plan"))]
  {
    let _ = (label, plan);
  }
}
