//! Executes normalized single-file GeoParquet.

use anyhow::{Context, Result};
use engine::output_layout::resolved_output_paths;
use engine::parquet_write::{
  create_datafusion_parquet_options, parse_compression, write_single_file,
};

use crate::diagnostics::explain_stage_note;
use crate::geoparquet::{
  GeoMetadata, GeoMetadataInput, analyze_plain_target_extent, plain_output_dataframe,
  resolve_source, validate_covering_configuration,
};
use crate::optimized::multiscale::COVERING_BBOX_COLUMN;
use crate::output::ParquetMetadataSet;
use crate::output::reprojection::ReprojectionSpec;

use super::{PlainPipeline, SpatialPipelineResult};

impl PlainPipeline {
  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    let state = self.state;
    validate_covering_configuration(state.covering, state.source_schema.as_ref())?;
    let source = resolve_source(
      state.input.as_ref(),
      state.input_dataframe.clone(),
      state.source_schema.as_ref(),
      state.geometry_column.as_deref(),
      state.input_wkid,
      state.row_range,
    )
    .await?;
    let source_projjson = source
      .source_spatial_reference
      .projjson
      .as_ref()
      .context("missing resolved source CRS PROJJSON")?;
    let reprojection = ReprojectionSpec::from_source_projjson(source_projjson, state.output_wkid)?;
    let target_extent = analyze_plain_target_extent(
      state.input_dataframe.clone(),
      &source.geometry_spec.column,
      source.geometry_shape.category(),
      reprojection.transform(),
    )
    .await?;
    let dataframe = plain_output_dataframe(
      state.input_dataframe.clone(),
      state.source_schema.as_ref(),
      &source.geometry_spec.column,
      source.geometry_shape.category(),
      reprojection.transform(),
      state.covering,
    )?;
    let geo_metadata = GeoMetadata::new(GeoMetadataInput {
      geometry_column: &source.geometry_spec.column,
      geometry_types: &source.geometry_types,
      output_extent: target_extent,
      output_spatial_reference: reprojection.target_spatial_reference(),
      has_z: source.has_z,
      has_m: source.has_m,
      covering: state.covering,
      covering_column: COVERING_BBOX_COLUMN,
    })?;
    let mut metadata = ParquetMetadataSet::new(source.source_metadata.passthrough_kv);
    metadata.insert(&geo_metadata)?;
    let metadata = metadata.into_entries();
    let compression = parse_compression(state.compression.as_deref().unwrap_or("snappy"))?;
    let writer_options = create_datafusion_parquet_options(compression, &metadata);
    let output_path = resolved_output_paths(&state.output_layout)?
      .into_iter()
      .next()
      .context("missing output path")?
      .to_string_lossy()
      .into_owned();
    let rows_written = write_single_file(dataframe, &output_path, writer_options).await?;
    explain_stage_note(
      state.explain,
      "Plain GeoParquet",
      &format!("wrote {rows_written} selected rows without optimized clustering"),
    );
    Ok(state.finish(rows_written))
  }
}
