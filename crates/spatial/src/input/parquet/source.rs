//! Adapts local and HTTP Parquet locations into footer-backed metadata and DataFusion scans.
//!
//! Retains file metadata and location state at open time, then registers HTTP object stores or
//! reads local paths when DataFusion executes the selected row range.

use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_schema::SchemaRef;
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;
use futures_util::future::BoxFuture;
use geoparquet::metadata::GeoParquetColumnEncoding;
use object_store::ObjectStore;
use parquet::arrow::arrow_reader::ArrowReaderMetadata;
use url::Url;

use crate::geometry::{GeometryEncoding, GeometrySpec};
use crate::input::{InputSource, RowRange, SourceDatasetMetadata, SourceGeometryMetadata};

use super::metadata::{
  load_covering_metadata, load_geo_metadata, map_geo_geometry_type, passthrough_metadata,
};

/// Stores Parquet footer metadata and the location needed to construct future scans.
pub(super) struct ParquetInputSource {
  location: ParquetInputLocation,
  metadata: Vec<ArrowReaderMetadata>,
}

/// Distinguishes local DataFusion paths from registered HTTP object-store locations.
pub(super) enum ParquetInputLocation {
  Local {
    input_path: String,
  },
  Http {
    input_url: String,
    store_url: Url,
    store: Arc<dyn ObjectStore>,
  },
}

impl ParquetInputSource {
  pub(super) fn new(location: ParquetInputLocation, metadata: Vec<ArrowReaderMetadata>) -> Self {
    Self { location, metadata }
  }
}

impl InputSource for ParquetInputSource {
  fn schema(&self) -> Result<SchemaRef> {
    let metadata = self
      .metadata
      .first()
      .context("parquet input requires at least one file")?;
    Ok(metadata.schema().clone())
  }

  fn total_rows(&self) -> Result<u64> {
    Ok(
      self
        .metadata
        .iter()
        .map(|metadata| {
          metadata
            .metadata()
            .row_groups()
            .iter()
            .map(|row_group| row_group.num_rows() as u64)
            .sum::<u64>()
        })
        .sum(),
    )
  }

  fn inferred_geometry_spec(&self) -> Result<Option<GeometrySpec>> {
    let Some(geo_meta) = load_geo_metadata(&self.metadata)? else {
      return Ok(None);
    };
    let Some(column_meta) = geo_meta.columns.get(&geo_meta.primary_column) else {
      return Ok(None);
    };
    if column_meta.encoding != GeoParquetColumnEncoding::WKB {
      return Ok(None);
    }

    let geometry_kind = if column_meta.geometry_types.len() == 1 {
      let geo_type = column_meta.geometry_types.iter().next().unwrap();
      Some(map_geo_geometry_type(geo_type.geometry_type()))
    } else {
      None
    };

    Ok(Some(GeometrySpec {
      column: geo_meta.primary_column.clone(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind,
    }))
  }

  fn source_metadata(&self) -> Result<SourceDatasetMetadata> {
    let geometry = match load_geo_metadata(&self.metadata)? {
      Some(geo_meta) => {
        let covering = load_covering_metadata(&self.metadata, &geo_meta.primary_column)?;
        SourceGeometryMetadata::from_geoparquet(&geo_meta, covering)?
      }
      None => None,
    };

    Ok(SourceDatasetMetadata {
      geometry,
      passthrough_kv: passthrough_metadata(&self.metadata),
    })
  }

  fn to_dataframe<'a>(
    &'a self,
    ctx: &'a SessionContext,
    row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>> {
    match &self.location {
      ParquetInputLocation::Local { input_path } => {
        let input_path = input_path.clone();
        Box::pin(async move {
          let mut dataframe = ctx.read_parquet(&input_path, Default::default()).await?;
          if !row_range.is_full() {
            dataframe = dataframe.limit(row_range.start(), row_range.num())?;
          }
          Ok(dataframe)
        })
      }
      ParquetInputLocation::Http {
        input_url,
        store_url,
        store,
        ..
      } => {
        let input_url = input_url.clone();
        let store_url = store_url.clone();
        let store = Arc::clone(store);
        Box::pin(async move {
          ctx.register_object_store(&store_url, store);
          let mut dataframe = ctx.read_parquet(&input_url, Default::default()).await?;
          if !row_range.is_full() {
            dataframe = dataframe.limit(row_range.start(), row_range.num())?;
          }
          Ok(dataframe)
        })
      }
    }
  }
}
