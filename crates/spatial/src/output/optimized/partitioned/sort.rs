// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Inserts range repartitioning and partition-local sorting for multi-file output.
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
pub(super) struct PartitionedSortConfig {
  partition_column: String,
  cluster_key_column: String,
  bucket_count: usize,
  drop_cluster_key_after_sort: bool,
}

impl PartitionedSortConfig {
  /// Construct range partitioning and partition-local ordering configuration.
  pub(super) fn new(
    partition_column: impl Into<String>,
    cluster_key_column: impl Into<String>,
    bucket_count: usize,
    drop_cluster_key_after_sort: bool,
  ) -> Self {
    Self {
      partition_column: partition_column.into(),
      cluster_key_column: cluster_key_column.into(),
      bucket_count,
      drop_cluster_key_after_sort,
    }
  }

  /// Insert range repartitioning and spatial sorting into one physical plan.
  pub(super) fn insert_into(
    &self,
    plan: Arc<dyn ExecutionPlan>,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    self.insert_partitioned_sort_exec(plan)
  }

  fn insert_partitioned_sort_exec(
    &self,
    plan: Arc<dyn ExecutionPlan>,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let schema = plan.schema();
    let partition_column_index = schema.index_of(&self.partition_column);
    let cluster_key_column_index = schema.index_of(&self.cluster_key_column);
    if let (Ok(partition_column_index), Ok(cluster_key_column_index)) =
      (partition_column_index, cluster_key_column_index)
    {
      return self.partitioned_sort_exec(plan, partition_column_index, cluster_key_column_index);
    }
    let children = plan.children();
    if children.is_empty() {
      return Err(DataFusionError::Execution(format!(
        "unable to insert partitioned sort: columns '{}' and '{}' were not both available in plan schema {:?}",
        self.partition_column,
        self.cluster_key_column,
        schema
          .fields()
          .iter()
          .map(|field| field.name().clone())
          .collect::<Vec<_>>(),
      )));
    }
    let rewritten_children = children
      .into_iter()
      .map(|child| self.insert_partitioned_sort_exec(Arc::clone(child)))
      .collect::<DataFusionResult<Vec<_>>>()?;
    plan.with_new_children(rewritten_children)
  }

  fn partitioned_sort_exec(
    &self,
    plan: Arc<dyn ExecutionPlan>,
    partition_column_index: usize,
    cluster_key_column_index: usize,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let repartitioned_input: Arc<dyn ExecutionPlan> = Arc::new(RepartitionExec::try_new(
      plan,
      Partitioning::Hash(
        vec![Arc::new(PhysicalColumn::new(
          &self.partition_column,
          partition_column_index,
        ))],
        Self::partition_count(self.bucket_count),
      ),
    )?);
    let sort_order: LexOrdering = [PhysicalSortExpr {
      expr: Arc::new(PhysicalColumn::new(
        &self.cluster_key_column,
        cluster_key_column_index,
      )),
      options: SortOptions {
        descending: false,
        nulls_first: false,
      },
    }]
    .into();
    let sorted_input: Arc<dyn ExecutionPlan> =
      Arc::new(SortExec::new(sort_order, repartitioned_input).with_preserve_partitioning(true));
    if !self.drop_cluster_key_after_sort {
      return Ok(sorted_input);
    }
    let projection_exprs = sorted_input
      .schema()
      .fields()
      .iter()
      .enumerate()
      .filter(|(_, field)| field.name() != &self.cluster_key_column)
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

  fn partition_count(bucket_count: usize) -> usize {
    bucket_count.saturating_mul(2).max(8)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use arrow_array::{RecordBatch, UInt64Array};
  use arrow_schema::{DataType, Field, Schema};
  use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};

  use crate::optimized::{ClusteringFamily, GEOKEY_COLUMN};
  use crate::session::DataFusionSession;

  #[test]
  fn preserves_partitioned_sort_for_multi_file_writes() {
    let point_range_column = ClusteringFamily::PointGeometry.cluster_partition_column();
    let batch = RecordBatch::try_new(
      Arc::new(Schema::new(vec![
        Field::new(GEOKEY_COLUMN, DataType::UInt64, false),
        Field::new(point_range_column, DataType::UInt64, false),
      ])),
      vec![
        Arc::new(UInt64Array::from(vec![4_u64, 1, 3, 2])),
        Arc::new(UInt64Array::from(vec![10_u64, 0, 10, 0])),
      ],
    )
    .unwrap();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
      let session = DataFusionSession::new(None, None).unwrap();
      let dataframe = session.context().read_batch(batch).unwrap();
      let physical_plan = dataframe.create_physical_plan().await.unwrap();
      let rewritten = PartitionedSortConfig::new(point_range_column, GEOKEY_COLUMN, 2, false)
        .insert_into(physical_plan)
        .unwrap();

      let mut saw_sort = false;
      let mut saw_repartition = false;
      rewritten
        .apply(|plan| {
          if let Some(sort) = plan.downcast_ref::<SortExec>() {
            saw_sort = true;
            assert!(sort.preserve_partitioning());
          }
          if let Some(repartition) = plan.downcast_ref::<RepartitionExec>() {
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
