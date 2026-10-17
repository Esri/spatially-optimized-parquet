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

//! Executes partition-local Parquet sinks concurrently.

use std::fmt;
use std::sync::Arc;

use arrow_array::{RecordBatch, UInt64Array};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::datasource::sink::DataSink;
use datafusion::execution::TaskContext;
use datafusion::physical_expr::{Distribution, EquivalenceProperties};
use datafusion::physical_plan::{
  DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, Partitioning,
  PlanProperties, SendableRecordBatchStream, execute_input_stream,
  execution_plan::{EvaluationType, SchedulingType},
  stream::RecordBatchStreamAdapter,
};
use tokio::task::JoinSet;

use super::tracking_sink::TrackingSink;

#[derive(Clone, Debug)]
pub(super) struct PartitionPlan {
  input: Arc<dyn ExecutionPlan>,
  sink: Arc<TrackingSink>,
  count_schema: SchemaRef,
  cache: Arc<PlanProperties>,
}

impl PartitionPlan {
  /// Configure a concurrent partition sink that returns one aggregate row count.
  pub(super) fn new(input: Arc<dyn ExecutionPlan>, sink: Arc<TrackingSink>) -> Self {
    let count_schema = Self::count_schema();
    let cache = PlanProperties::new(
      EquivalenceProperties::new(Arc::clone(&count_schema)),
      Partitioning::UnknownPartitioning(1),
      input.pipeline_behavior(),
      input.boundedness(),
    )
    .with_scheduling_type(SchedulingType::Cooperative)
    .with_evaluation_type(EvaluationType::Eager);
    Self {
      input,
      sink,
      count_schema,
      cache: Arc::new(cache),
    }
  }

  /// Write all input partitions concurrently and return the first error after draining tasks.
  async fn run_concurrent_partition_writes(
    input: Arc<dyn ExecutionPlan>,
    sink: Arc<TrackingSink>,
    context: &Arc<TaskContext>,
  ) -> DataFusionResult<u64> {
    let mut write_tasks = JoinSet::new();
    for partition in 0..input.output_partitioning().partition_count() {
      let input = Arc::clone(&input);
      let sink = Arc::clone(&sink);
      let context = Arc::clone(context);
      write_tasks.spawn(async move {
        let data = execute_input_stream(
          input,
          Arc::clone(sink.schema()),
          partition,
          Arc::clone(&context),
        )?;
        sink.write_all(data, &context).await
      });
    }

    let mut rows_written = 0;
    let mut first_error = None;
    while let Some(result) = write_tasks.join_next().await {
      match result {
        Ok(Ok(count)) => rows_written += count,
        Ok(Err(error)) => {
          first_error.get_or_insert(error);
        }
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(error) => {
          first_error.get_or_insert_with(|| {
            DataFusionError::Execution(format!("partitioned parquet write task failed: {error}"))
          });
        }
      }
    }
    first_error.map_or(Ok(rows_written), Err)
  }

  /// Build the single-column schema for the aggregate written-row count.
  fn count_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new(
      "count",
      DataType::UInt64,
      false,
    )]))
  }

  /// Build the one-row batch reporting the completed partition write count.
  fn make_count_batch(count: u64) -> RecordBatch {
    RecordBatch::try_new(
      Self::count_schema(),
      vec![Arc::new(UInt64Array::from(vec![count]))],
    )
    .expect("count batch should always be valid")
  }
}

impl DisplayAs for PartitionPlan {
  /// Display the tracked Parquet sink behind the partition writer.
  fn fmt_as(
    &self,
    format_type: DisplayFormatType,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    self.sink.fmt_as(format_type, formatter)
  }
}

impl ExecutionPlan for PartitionPlan {
  /// Return the physical plan name for concurrent partition writes.
  fn name(&self) -> &'static str {
    "PartitionPlan"
  }

  /// Return cached execution properties for the single count-result partition.
  fn properties(&self) -> &Arc<PlanProperties> {
    &self.cache
  }

  /// Accept the partition distribution prepared by the sorting plan.
  fn required_input_distribution(&self) -> Vec<Distribution> {
    vec![Distribution::UnspecifiedDistribution]
  }

  /// Preserve input ordering within each independently written partition.
  fn maintains_input_order(&self) -> Vec<bool> {
    vec![true]
  }

  /// Expose the partitioned input plan for optimizer traversal.
  fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
    vec![&self.input]
  }

  /// Rebuild concurrent writing around the replacement input and shared sink.
  fn with_new_children(
    self: Arc<Self>,
    children: Vec<Arc<dyn ExecutionPlan>>,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    Ok(Arc::new(Self::new(
      Arc::clone(&children[0]),
      Arc::clone(&self.sink),
    )))
  }

  /// Execute all sink writes from partition zero and stream their aggregate count.
  fn execute(
    &self,
    partition: usize,
    context: Arc<TaskContext>,
  ) -> DataFusionResult<SendableRecordBatchStream> {
    if partition != 0 {
      return Err(DataFusionError::Execution(format!(
        "{} can only execute partition 0",
        self.name()
      )));
    }
    let count_schema = Arc::clone(&self.count_schema);
    let input = Arc::clone(&self.input);
    let sink = Arc::clone(&self.sink);
    let stream = futures_util::stream::once(async move {
      Self::run_concurrent_partition_writes(input, sink, &context)
        .await
        .map(Self::make_count_batch)
    });
    Ok(Box::pin(RecordBatchStreamAdapter::new(
      count_schema,
      stream,
    )))
  }
}
