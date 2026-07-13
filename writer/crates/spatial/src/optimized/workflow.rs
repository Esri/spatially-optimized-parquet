use arrow_array::RecordBatch;
use arrow_schema::Schema;
use engine::output_layout::OutputLayout;

use crate::input::{InputSource, RowRange};

/// Carries validated source and runtime controls into optimized output execution.
pub(crate) struct OptimizeOutputRequest<'a> {
  pub(crate) input: &'a dyn InputSource,
  pub(crate) session: &'a engine::SessionContext,
  pub(crate) output_layout: &'a OutputLayout,
  pub(crate) source_schema: &'a Schema,
  pub(crate) total_input_rows: u64,
  pub(crate) row_range: RowRange,
  pub(crate) materialized_batches: Option<&'a [RecordBatch]>,
  pub(crate) geometry_column: Option<&'a str>,
  pub(crate) input_wkid: Option<u32>,
  pub(crate) output_wkid: u32,
  pub(crate) covering: bool,
  pub(crate) compression: Option<&'a str>,
  pub(crate) progress: bool,
  pub(crate) explain: bool,
}
