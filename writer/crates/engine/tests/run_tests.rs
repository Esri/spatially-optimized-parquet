use std::sync::{Arc, Mutex};

use arrow_array::{Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use datafusion::prelude::SessionContext;
use engine::plan::validate_output;
use engine::run::{RunConfig, RunStatus, run_ordered_df};
use engine::{Compression, KeyValue, SchemaRef};
use tempfile::TempDir;

#[tokio::test]
async fn run_ordered_df_reports_stage_transitions() {
  let schema: SchemaRef = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));
  let batch =
    RecordBatch::try_new(schema.clone(), vec![Arc::new(Int32Array::from(vec![2, 1]))]).unwrap();
  let ctx = SessionContext::new();
  let df = ctx.read_batch(batch).unwrap();
  let temp = TempDir::new().unwrap();
  let output_path = temp.path().join("out.parquet");
  let output_plan = validate_output(&output_path, None, false).unwrap();
  let statuses = Arc::new(Mutex::new(Vec::new()));
  let captured_statuses = statuses.clone();
  let on_status = move |status| {
    captured_statuses.lock().unwrap().push(status);
  };
  let metadata: Vec<KeyValue> = Vec::new();

  run_ordered_df(
    df,
    RunConfig {
      output_schema: &schema,
      output_plan: &output_plan,
      compression: Compression::SNAPPY,
      total_rows: 2,
      kv_metadata: &metadata,
      transform: |batch, _| Ok(batch.clone()),
      on_status: Some(&on_status),
    },
  )
  .await
  .unwrap();

  assert_eq!(
    *statuses.lock().unwrap(),
    vec![
      RunStatus::WaitingForBatch,
      RunStatus::TransformingBatch { rows: 2 },
      RunStatus::WritingBatch {
        rows: 2,
        file_index: 1,
        file_count: 1,
      },
      RunStatus::BatchWritten {
        rows: 2,
        file_index: 1,
        file_count: 1,
      },
      RunStatus::WaitingForBatch,
      RunStatus::FinalizingOutputs,
    ]
  );
}
