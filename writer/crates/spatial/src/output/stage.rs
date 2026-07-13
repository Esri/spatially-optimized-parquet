use anyhow::Result;
use arrow_schema::Schema;
use async_trait::async_trait;
use engine::output_layout::OutputLayout;

use crate::input::{InputSource, RowRange};

/// Stores validated job resources and controls for one output stage.
pub(crate) struct OutputStageContext<'a> {
  pub(crate) input: &'a dyn InputSource,
  pub(crate) input_dataframe: engine::DataFrame,
  pub(crate) output_layout: &'a OutputLayout,
  pub(crate) source_schema: &'a Schema,
  pub(crate) total_input_rows: u64,
  pub(crate) row_range: RowRange,
  pub(crate) geometry_column: Option<&'a str>,
  pub(crate) input_wkid: Option<u32>,
  pub(crate) output_wkid: u32,
  pub(crate) covering: bool,
  pub(crate) compression: Option<&'a str>,
  pub(crate) progress: bool,
  pub(crate) explain: bool,
}

/// Represents the durable output produced by an output stage.
pub(crate) struct OutputStageResult {
  pub(crate) rows_written: u64,
}

/// Defines one statically dispatched GeoParquet output pipeline.
#[async_trait]
pub(crate) trait OutputStage {
  /// Write one GeoParquet product from validated job resources.
  async fn execute(&self, context: OutputStageContext<'_>) -> Result<OutputStageResult>;
}
