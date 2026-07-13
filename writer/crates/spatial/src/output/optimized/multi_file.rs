//! Composes the custom DataFusion plan required for multi-file SOP output.
//!
//! DataFusion provides repartition, sort, and Parquet sink operators, but it does not expose
//! Spark's combined `repartition(...).sortWithinPartitions(...).write.partitionBy(...)` workflow
//! with SOP's exact file-count and output-schema guarantees. This module composes DataFusion
//! operators to provide those guarantees. DataFusion still executes every operator and performs
//! Parquet encoding.

use std::sync::Arc;

use arrow_schema::SortOptions;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::physical_expr::expressions::Column as PhysicalColumn;
use datafusion::physical_expr::{LexOrdering, PhysicalSortExpr};
use datafusion::physical_plan::{
  ExecutionPlan, Partitioning,
  projection::{ProjectionExec, ProjectionExpr},
  repartition::RepartitionExec,
  sorts::sort::SortExec,
};

/// Describes range partitioning and partition-local ordering for multi-file SOP output.
#[derive(Clone)]
pub(crate) struct MultiFileWriteConfig {
  pub(crate) partition_column: String,
  pub(crate) sort_column: String,
  pub(crate) bucket_count: usize,
  pub(crate) drop_sort_column_after_sort: bool,
}

/// Insert range repartitioning and spatial sorting where both control columns are available.
pub(crate) fn preserve_partitioned_sort_execs(
  plan: Arc<dyn ExecutionPlan>,
  config: &MultiFileWriteConfig,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
  insert_partitioned_sort_exec(plan, config)
}

fn insert_partitioned_sort_exec(
  plan: Arc<dyn ExecutionPlan>,
  config: &MultiFileWriteConfig,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
  let schema = plan.schema();
  let partition_column_index = schema.index_of(&config.partition_column);
  let sort_column_index = schema.index_of(&config.sort_column);
  if let (Ok(partition_column_index), Ok(sort_column_index)) =
    (partition_column_index, sort_column_index)
  {
    return build_partitioned_sort_exec(plan, config, partition_column_index, sort_column_index);
  }
  let children = plan.children();
  if children.is_empty() {
    return Err(DataFusionError::Execution(format!(
      "unable to insert partitioned sort: columns '{}' and '{}' were not both available in plan schema {:?}",
      config.partition_column,
      config.sort_column,
      schema
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect::<Vec<_>>(),
    )));
  }
  let rewritten_children = children
    .into_iter()
    .map(|child| insert_partitioned_sort_exec(Arc::clone(child), config))
    .collect::<DataFusionResult<Vec<_>>>()?;
  plan.with_new_children(rewritten_children)
}

fn build_partitioned_sort_exec(
  plan: Arc<dyn ExecutionPlan>,
  config: &MultiFileWriteConfig,
  partition_column_index: usize,
  sort_column_index: usize,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
  let repartitioned_input: Arc<dyn ExecutionPlan> = Arc::new(RepartitionExec::try_new(
    plan,
    Partitioning::Hash(
      vec![Arc::new(PhysicalColumn::new(
        &config.partition_column,
        partition_column_index,
      ))],
      multi_file_sort_partition_count(config.bucket_count),
    ),
  )?);
  let sort_order: LexOrdering = [PhysicalSortExpr {
    expr: Arc::new(PhysicalColumn::new(&config.sort_column, sort_column_index)),
    options: SortOptions {
      descending: false,
      nulls_first: false,
    },
  }]
  .into();
  let sorted_input: Arc<dyn ExecutionPlan> =
    Arc::new(SortExec::new(sort_order, repartitioned_input).with_preserve_partitioning(true));
  if !config.drop_sort_column_after_sort {
    return Ok(sorted_input);
  }
  let projection_exprs = sorted_input
    .schema()
    .fields()
    .iter()
    .enumerate()
    .filter(|(_, field)| field.name() != &config.sort_column)
    .map(|(index, field)| ProjectionExpr {
      expr: Arc::new(PhysicalColumn::new(field.name(), index)),
      alias: field.name().to_string(),
    })
    .collect::<Vec<_>>();
  Ok(Arc::new(ProjectionExec::try_new(
    projection_exprs,
    sorted_input,
  )?))
}

fn multi_file_sort_partition_count(bucket_count: usize) -> usize {
  bucket_count.saturating_mul(2).max(8)
}

#[cfg(test)]
mod tests {
  use super::*;
  use arrow_array::{RecordBatch, UInt64Array};
  use arrow_schema::{DataType, Field, Schema};
  use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
  use engine::session::new_datafusion_session;

  use crate::output::optimized::multiscale::POINT_Z_CODE_COLUMN;
  use crate::output::optimized::plan::POINT_RANGE_COLUMN;

  #[test]
  fn preserves_partitioned_sort_for_multi_file_writes() {
    let batch = RecordBatch::try_new(
      Arc::new(Schema::new(vec![
        Field::new(POINT_Z_CODE_COLUMN, DataType::UInt64, false),
        Field::new(POINT_RANGE_COLUMN, DataType::UInt64, false),
      ])),
      vec![
        Arc::new(UInt64Array::from(vec![4_u64, 1, 3, 2])),
        Arc::new(UInt64Array::from(vec![10_u64, 0, 10, 0])),
      ],
    )
    .unwrap();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
      let session = new_datafusion_session().unwrap();
      let dataframe = session.context().read_batch(batch).unwrap();
      let physical_plan = dataframe.create_physical_plan().await.unwrap();
      let rewritten = preserve_partitioned_sort_execs(
        physical_plan,
        &MultiFileWriteConfig {
          partition_column: POINT_RANGE_COLUMN.to_string(),
          sort_column: POINT_Z_CODE_COLUMN.to_string(),
          bucket_count: 2,
          drop_sort_column_after_sort: false,
        },
      )
      .unwrap();

      let mut saw_sort = false;
      let mut saw_repartition = false;
      rewritten
        .apply(|plan| {
          if let Some(sort) = plan.as_any().downcast_ref::<SortExec>() {
            saw_sort = true;
            assert!(sort.preserve_partitioning());
          }
          if let Some(repartition) = plan.as_any().downcast_ref::<RepartitionExec>() {
            saw_repartition = true;
            assert!(matches!(
              repartition.partitioning(),
              Partitioning::Hash(_, partition_count) if *partition_count > 1
            ));
          }
          Ok(TreeNodeRecursion::Continue)
        })
        .unwrap();
      assert!(saw_sort);
      assert!(saw_repartition);
    });
  }
}
