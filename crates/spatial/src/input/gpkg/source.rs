//! Adapts one GeoPackage layer into normalized source metadata and a DataFusion table.
//!
//! Captures GDAL-derived layer state at open time, then creates partitioned Arrow streams when
//! DataFusion executes the selected row range.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use datafusion::catalog::streaming::StreamingTable;
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::streaming::PartitionStream;
use futures_util::future::BoxFuture;
use gdal::vector::LayerAccess;

use super::batch_reader::GpkgBatchReader;
use super::metadata::GpkgLayerSummary;
use super::open::{is_gpkg_path, open_gpkg_dataset};
use super::partition::{GpkgPartitionStream, GpkgScanPartition};
use crate::geometry::{GeometryColumn, GeometryEncoding};
use crate::input::{
  InputOpenOptions, InputSource, RowRange, SourceDatasetMetadata, SourceGeometryMetadata,
};

#[derive(Debug, Clone)]
/// Represents normalized GeoPackage metadata and constructs GDAL-backed batch streams.
pub(crate) struct GpkgInputSource {
  input_path: PathBuf,
  layer_name: String,
  schema: arrow_schema::SchemaRef,
  total_rows: u64,
  geometry: Option<GeometryColumn>,
  source_metadata: SourceDatasetMetadata,
}

impl InputSource for GpkgInputSource {
  fn schema(&self) -> Result<arrow_schema::SchemaRef> {
    Ok(self.schema.clone())
  }

  fn total_rows(&self) -> Result<u64> {
    Ok(self.total_rows)
  }

  fn inferred_geometry_column(&self) -> Result<Option<GeometryColumn>> {
    Ok(self.geometry.clone())
  }

  fn source_metadata(&self) -> Result<SourceDatasetMetadata> {
    Ok(self.source_metadata.clone())
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
      let partitions = GpkgScanPartition::plan(
        &input_path,
        &layer_name,
        total_rows,
        row_range,
        ctx.copied_config().target_partitions(),
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

impl GpkgInputSource {
  /// Open one local GeoPackage layer through GDAL.
  pub(crate) async fn open(options: &InputOpenOptions) -> Result<Arc<dyn InputSource>> {
    let path = options
      .local_path()
      .ok_or_else(|| anyhow::anyhow!("gpkg input does not support HTTP locations"))?;
    if !is_gpkg_path(path) {
      bail!("GeoPackage input must use a .gpkg file: {}", path.display());
    }

    let dataset = open_gpkg_dataset(path)?;
    let layer_summaries = GpkgLayerSummary::collect(&dataset)?;
    let layer_name = GpkgLayerSummary::select_name(options, &layer_summaries)?;
    let mut layer = dataset
      .layer_by_name(&layer_name)
      .with_context(|| format!("failed to open GeoPackage layer {layer_name}"))?;
    let geometry_metadata = SourceGeometryMetadata::from_gpkg_layer(&mut layer, &layer_name)?;
    let schema = GpkgBatchReader::load_schema(path, &layer_name, &geometry_metadata.column)?;
    let total_rows = layer
      .try_feature_count()
      .unwrap_or_else(|| layer.feature_count());
    let geometry_kind = geometry_metadata.geometry_types.first().copied();
    let geometry = Some(GeometryColumn {
      column: geometry_metadata.column.clone(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind,
    });
    let source_metadata = SourceDatasetMetadata {
      geometry: Some(geometry_metadata),
      passthrough_kv: Vec::new(),
    };

    Ok(Arc::new(Self {
      input_path: path.to_path_buf(),
      layer_name,
      schema,
      total_rows,
      geometry,
      source_metadata,
    }))
  }
}
