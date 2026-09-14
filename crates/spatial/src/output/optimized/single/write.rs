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

//! Persists globally ordered optimized output into one Parquet file.

use crate::optimized::OptimizedLayout;
use crate::output::{OutputError, OutputPath, Writer, WriterOptions};
use crate::pipeline::{PipelineWarnings, SharedWriteReporter, SpatialWriteContext};

use super::dataframe;

/// Write one globally ordered optimized GeoParquet file.
pub(crate) async fn write(
  output_path: &OutputPath,
  source_schema: &arrow_schema::Schema,
  context: &SpatialWriteContext,
  layout: &OptimizedLayout,
  covering: bool,
  compression: Option<&str>,
  total_input_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
  warnings: PipelineWarnings,
) -> Result<u64, OutputError> {
  let dataframe = dataframe::dataframe(source_schema, context, layout, covering, warnings)?;
  let metadata = layout
    .parquet_metadata(context, covering)
    .map_err(|error| OutputError::Configuration(format!("build optimized metadata: {error}")))?;
  let geometry_crs = format!(
    "srid:{}",
    context
      .reprojection()
      .target_spatial_reference()
      .wkid
      .ok_or_else(|| {
        OutputError::Configuration("missing output spatial-reference WKID".to_string())
      })?
  );
  let writer_options = WriterOptions::new(compression.unwrap_or("snappy"), &metadata)?
    .with_byte_stream_split_columns(layout.byte_stream_split_column_paths())
    .with_geometry_column(&layout.geometry().geometry.column, geometry_crs);
  let output_path = output_path
    .paths()?
    .into_iter()
    .next()
    .ok_or_else(|| OutputError::Configuration("missing output path".to_string()))?
    .to_string_lossy()
    .into_owned();
  Writer::new(total_input_rows, write_reporter)
    .write_single(dataframe, output_path, writer_options, Vec::new())
    .await
}
