use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_schema::SchemaRef;
use engine::read::{execute_stream, read_parquet_df};
use engine::{DataFrame, SessionContext};
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use geoparquet::metadata::GeoParquetColumnEncoding;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectMeta, ObjectStore};
use parquet::arrow::ParquetRecordBatchStreamBuilder;
use parquet::arrow::arrow_reader::ArrowReaderMetadata;
use parquet::arrow::async_reader::ParquetObjectReader;
use parquet::file::metadata::KeyValue;
use url::Url;

use crate::geometry::{GeometryEncoding, GeometrySpec};
use crate::input::{InputBatchStream, InputSource, RowRange};
use crate::metadata::source::SourceDatasetMetadata;

use super::metadata::{
  build_source_geometry_metadata, file_metadata, load_geo_metadata, map_geo_geometry_type,
  passthrough_metadata,
};

/// Stores Parquet footer metadata and the location needed to construct future scans.
pub struct ParquetInputSource {
  location: ParquetInputLocation,
  source_location: String,
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
    object_path: ObjectPath,
    object_meta: ObjectMeta,
  },
}

impl ParquetInputSource {
  pub(super) fn new(
    location: ParquetInputLocation,
    source_location: String,
    metadata: Vec<ArrowReaderMetadata>,
  ) -> Self {
    Self {
      location,
      source_location,
      metadata,
    }
  }
}

impl InputSource for ParquetInputSource {
  fn format_name(&self) -> &'static str {
    "parquet"
  }

  fn source_location(&self) -> &str {
    &self.source_location
  }

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
      Some(geo_meta) => build_source_geometry_metadata(&geo_meta)?,
      None => None,
    };

    Ok(SourceDatasetMetadata {
      geometry,
      passthrough_kv: passthrough_metadata(&self.metadata),
    })
  }

  fn file_metadata(&self) -> Result<Vec<KeyValue>> {
    Ok(file_metadata(&self.metadata))
  }

  fn read_batches(&self, row_range: RowRange) -> BoxFuture<'_, Result<InputBatchStream>> {
    match &self.location {
      ParquetInputLocation::Local { input_path } => {
        let input_path = input_path.clone();
        Box::pin(async move {
          let mut dataframe = read_parquet_df(&input_path).await?;
          if !row_range.is_full() {
            dataframe = dataframe.limit(row_range.start, row_range.num)?;
          }

          let stream = execute_stream(dataframe).await?;
          Ok(Box::pin(stream.map(|batch| batch.map_err(Into::into))) as InputBatchStream)
        })
      }
      ParquetInputLocation::Http {
        store,
        object_path,
        object_meta,
        ..
      } => {
        let store = Arc::clone(store);
        let object_path = object_path.clone();
        let object_meta = object_meta.clone();
        let metadata = self.metadata[0].clone();
        Box::pin(async move {
          let reader =
            ParquetObjectReader::new(store, object_path).with_file_size(object_meta.size);
          let mut builder = ParquetRecordBatchStreamBuilder::new_with_metadata(reader, metadata);
          builder = builder.with_offset(row_range.start);
          if let Some(num) = row_range.num {
            builder = builder.with_limit(num);
          }
          let stream = builder.build()?;
          Ok(Box::pin(stream.map(|batch| batch.map_err(Into::into))) as InputBatchStream)
        })
      }
    }
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
            dataframe = dataframe.limit(row_range.start, row_range.num)?;
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
            dataframe = dataframe.limit(row_range.start, row_range.num)?;
          }
          Ok(dataframe)
        })
      }
    }
  }
}
