//! Attaches the GeoArrow WKB extension contract to the primary geometry field.

use std::fmt;
use std::sync::Arc;

use arrow_array::RecordBatch;
use arrow_schema::{Schema, SchemaRef};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::TaskContext;
use datafusion::physical_expr::{Distribution, EquivalenceProperties};
use datafusion::physical_plan::{
  DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, PlanProperties,
  SendableRecordBatchStream,
  execution_plan::{EvaluationType, SchedulingType},
  stream::RecordBatchStreamAdapter,
};
use futures_util::StreamExt;
use parquet_geospatial::{WkbMetadata, WkbType};

/// Annotates one WKB field so Parquet writes the native GEOMETRY logical type.
#[derive(Debug)]
pub(super) struct GeometrySchemaExec {
  input: Arc<dyn ExecutionPlan>,
  schema: SchemaRef,
  geometry_column: String,
  crs: String,
  cache: Arc<PlanProperties>,
}

impl GeometrySchemaExec {
  pub(super) fn try_new(
    input: Arc<dyn ExecutionPlan>,
    geometry_column: &str,
    crs: &str,
  ) -> DataFusionResult<Self> {
    let fields = input
      .schema()
      .fields()
      .iter()
      .map(|field| {
        if field.name() == geometry_column {
          Arc::new(
            field
              .as_ref()
              .clone()
              .with_extension_type(WkbType::new(Some(WkbMetadata::new(Some(crs), None)))),
          )
        } else {
          Arc::clone(field)
        }
      })
      .collect::<Vec<_>>();
    if !fields.iter().any(|field| field.name() == geometry_column) {
      return Err(DataFusionError::Plan(format!(
        "geometry column '{geometry_column}' is missing from output schema"
      )));
    }
    let schema = Arc::new(Schema::new_with_metadata(
      fields,
      input.schema().metadata().clone(),
    ));
    let cache = PlanProperties::new(
      EquivalenceProperties::new(Arc::clone(&schema)),
      input.output_partitioning().clone(),
      input.pipeline_behavior(),
      input.boundedness(),
    )
    .with_scheduling_type(SchedulingType::Cooperative)
    .with_evaluation_type(EvaluationType::Eager);
    Ok(Self {
      input,
      schema,
      geometry_column: geometry_column.to_string(),
      crs: crs.to_string(),
      cache: Arc::new(cache),
    })
  }
}

impl DisplayAs for GeometrySchemaExec {
  fn fmt_as(
    &self,
    _format_type: DisplayFormatType,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    write!(formatter, "GeometrySchemaExec")
  }
}

impl ExecutionPlan for GeometrySchemaExec {
  fn name(&self) -> &'static str {
    "GeometrySchemaExec"
  }

  fn properties(&self) -> &Arc<PlanProperties> {
    &self.cache
  }

  fn required_input_distribution(&self) -> Vec<Distribution> {
    vec![Distribution::UnspecifiedDistribution]
  }

  fn maintains_input_order(&self) -> Vec<bool> {
    vec![true]
  }

  fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
    vec![&self.input]
  }

  fn with_new_children(
    self: Arc<Self>,
    children: Vec<Arc<dyn ExecutionPlan>>,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    Ok(Arc::new(Self::try_new(
      Arc::clone(&children[0]),
      &self.geometry_column,
      &self.crs,
    )?))
  }

  fn execute(
    &self,
    partition: usize,
    context: Arc<TaskContext>,
  ) -> DataFusionResult<SendableRecordBatchStream> {
    let schema = Arc::clone(&self.schema);
    let output_schema = Arc::clone(&schema);
    let stream = self.input.execute(partition, context)?.map(move |batch| {
      let batch = batch?;
      RecordBatch::try_new(Arc::clone(&schema), batch.columns().to_vec()).map_err(Into::into)
    });
    Ok(Box::pin(RecordBatchStreamAdapter::new(
      output_schema,
      stream,
    )))
  }
}
