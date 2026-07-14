use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use datafusion::catalog::streaming::StreamingTable;
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::streaming::PartitionStream;
use engine::DataFusionSession;
#[cfg(test)]
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use gdal::vector::LayerAccess;

use crate::geometry::{GeometryEncoding, GeometrySpec};
#[cfg(test)]
use crate::input::source::InputBatchStream;
use crate::input::{
  InputOpenOptions, InputSource, RowRange, SourceDatasetMetadata, SourceGeometryMetadata,
};

use super::batch_reader::load_schema;
#[cfg(test)]
use super::batch_reader::{batch_stream, open_gpkg_batch_reader};
use super::metadata::{collect_layer_summaries, select_layer_name};
use super::open::{is_gpkg_path, open_gpkg_dataset};
use super::partition::{GpkgPartitionStream, plan_gpkg_scan_partitions};

#[derive(Debug, Clone)]
/// Stores normalized GeoPackage metadata and constructs GDAL-backed batch streams.
struct GpkgInputSource {
  input_path: PathBuf,
  layer_name: String,
  schema: arrow_schema::SchemaRef,
  total_rows: u64,
  geometry_spec: Option<GeometrySpec>,
  source_metadata: SourceDatasetMetadata,
}

/// Open one local GeoPackage layer through GDAL.
pub(in crate::input) async fn open_source(
  options: &InputOpenOptions,
) -> Result<Arc<dyn InputSource>> {
  let path = options
    .local_path()
    .ok_or_else(|| anyhow::anyhow!("gpkg input does not support HTTP locations"))?;
  if !is_gpkg_path(path) {
    bail!("GeoPackage input must use a .gpkg file: {}", path.display());
  }

  let dataset = open_gpkg_dataset(path)?;
  let layer_summaries = collect_layer_summaries(&dataset)?;
  let layer_name = select_layer_name(options, &layer_summaries)?;
  let mut layer = dataset
    .layer_by_name(&layer_name)
    .with_context(|| format!("failed to open GeoPackage layer {layer_name}"))?;
  let geometry_metadata = SourceGeometryMetadata::from_gpkg_layer(&mut layer, &layer_name)?;
  let schema = load_schema(path, &layer_name, &geometry_metadata.column)?;
  let total_rows = layer
    .try_feature_count()
    .unwrap_or_else(|| layer.feature_count());
  let geometry_kind = geometry_metadata.geometry_types.first().copied();
  let geometry_spec = Some(GeometrySpec {
    column: geometry_metadata.column.clone(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind,
  });
  let source_metadata = SourceDatasetMetadata {
    geometry: Some(geometry_metadata),
    passthrough_kv: Vec::new(),
  };

  Ok(Arc::new(GpkgInputSource {
    input_path: path.to_path_buf(),
    layer_name,
    schema,
    total_rows,
    geometry_spec,
    source_metadata,
  }))
}

impl InputSource for GpkgInputSource {
  fn format_name(&self) -> &'static str {
    "GeoPackage"
  }

  fn schema(&self) -> Result<arrow_schema::SchemaRef> {
    Ok(self.schema.clone())
  }

  fn total_rows(&self) -> Result<u64> {
    Ok(self.total_rows)
  }

  fn inferred_geometry_spec(&self) -> Result<Option<GeometrySpec>> {
    Ok(self.geometry_spec.clone())
  }

  fn source_metadata(&self) -> Result<SourceDatasetMetadata> {
    Ok(self.source_metadata.clone())
  }

  #[cfg(test)]
  fn read_batches(&self, row_range: RowRange) -> BoxFuture<'_, Result<InputBatchStream>> {
    let input_path = self.input_path.clone();
    let layer_name = self.layer_name.clone();
    let schema = self.schema.clone();
    Box::pin(async move {
      let rows_to_read = row_range
        .num()
        .map(|num| num.saturating_add(row_range.start()));
      let reader = open_gpkg_batch_reader(&input_path, &layer_name, schema, None, rows_to_read)
        .with_context(|| format!("failed to stream GeoPackage layer {layer_name}"))?;
      let mut rows_to_skip = row_range.start();
      let stream = batch_stream(reader).filter_map(move |batch| {
        let out = match batch {
          Ok(batch) if rows_to_skip >= batch.num_rows() => {
            rows_to_skip -= batch.num_rows();
            None
          }
          Ok(batch) => {
            let offset = rows_to_skip;
            rows_to_skip = 0;
            Some(Ok(batch.slice(offset, batch.num_rows() - offset)))
          }
          Err(err) => Some(Err(err)),
        };
        std::future::ready(out)
      });
      Ok(Box::pin(stream) as InputBatchStream)
    })
  }

  fn to_dataframe<'a>(
    &'a self,
    ctx: &'a SessionContext,
    row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>> {
    let input_path = self.input_path.clone();
    let layer_name = self.layer_name.clone();
    let schema = self.schema.clone();
    let total_rows = self.total_rows;
    Box::pin(async move {
      let partitions = plan_gpkg_scan_partitions(
        &input_path,
        &layer_name,
        total_rows,
        row_range,
        DataFusionSession::target_partition_count(),
      )?;
      let streams: Vec<_> = partitions
        .into_iter()
        .map(|partition| {
          Arc::new(GpkgPartitionStream::new(
            input_path.clone(),
            layer_name.clone(),
            schema.clone(),
            partition.attribute_filter(),
          )) as Arc<dyn PartitionStream>
        })
        .collect();
      let table = StreamingTable::try_new(schema, streams)?;
      let mut df = ctx.read_table(Arc::new(table))?;
      if let Some(num) = row_range.num() {
        df = df.limit(0, Some(num))?;
      }
      Ok(df)
    })
  }
}
