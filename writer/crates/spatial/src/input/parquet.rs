//! Integrates local and HTTP Parquet sources, including GeoParquet metadata normalization.
//!
//! [`ParquetInputProvider`] accepts one local file, a directory of `.parquet` files, or a direct
//! HTTP(S) URL ending in `.parquet`. Discovery reads every local footer or performs HTTP object
//! metadata and footer range requests. GeoParquet `geo` JSON must remain semantically consistent
//! across a local file set. Reserved metadata stays under writer control, while unrelated
//! key-value pairs can pass through to output.
//!
//! Normal scans delegate to `SessionContext::read_parquet`, so DataFusion owns row-group/page
//! planning, decompression, partition scheduling, limits, and Arrow batch production. Direct
//! `read_batches` uses the same DataFusion path locally and a Parquet object reader over HTTP.
//! The job may materialize very small bounded HTTP ranges once, preventing its independent
//! analysis and write plans from repeating remote reads.
//!
//! Footer discovery cost scales with file count, and HTTP execution can issue new range requests
//! after provider discovery. The source stores loaded footer metadata so schema, row count, and
//! spatial metadata queries do not reopen local files.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_schema::SchemaRef;
use engine::read::{execute_stream, read_parquet_df};
use engine::{DataFrame, SessionContext};
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use geoparquet::metadata::{GeoParquetColumnEncoding, GeoParquetGeometryType, GeoParquetMetadata};
use object_store::http::HttpBuilder;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectMeta, ObjectStore};
use parquet::arrow::ParquetRecordBatchStreamBuilder;
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};
use parquet::arrow::async_reader::ParquetObjectReader;
use parquet::file::metadata::KeyValue;
use serde_json::Value;
use url::Url;

use crate::analysis::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryKind, GeometrySpec};
use crate::input::{
  InputBatchStream, InputOpenOptions, InputProvider, InputSource, RowRange, is_http_url,
};
use crate::metadata::source::{SourceDatasetMetadata, SourceGeometryMetadata};

/// Detects local Parquet files, Parquet directories, and direct HTTP Parquet URLs.
pub struct ParquetInputProvider;

/// Stores Parquet footer metadata and the location needed to construct future scans.
pub struct ParquetInputSource {
  location: ParquetInputLocation,
  source_location: String,
  metadata: Vec<ArrowReaderMetadata>,
}

/// Distinguishes local DataFusion paths from registered HTTP object-store locations.
enum ParquetInputLocation {
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

impl ParquetInputProvider {
  /// Build the stateless Parquet provider.
  pub fn new() -> Self {
    Self
  }
}

impl InputProvider for ParquetInputProvider {
  fn name(&self) -> &'static str {
    "parquet"
  }

  fn open<'a>(
    &'a self,
    options: &'a InputOpenOptions,
  ) -> BoxFuture<'a, Result<Option<Arc<dyn InputSource>>>> {
    Box::pin(async move {
      if is_http_url(&options.location) {
        if let Some(layer) = options.layer.as_deref() {
          return Err(anyhow::anyhow!(
            "parquet input does not support --layer (got '{layer}')"
          ));
        }
        return open_http_parquet(&options.location).await.map(Some);
      }

      let Some(path) = options.local_path() else {
        return Ok(None);
      };
      let Some(files) = discover_parquet_files(path)? else {
        return Ok(None);
      };
      if let Some(layer) = options.layer.as_deref() {
        return Err(anyhow::anyhow!(
          "parquet input does not support --layer (got '{layer}')"
        ));
      }
      let metadata = files
        .iter()
        .map(|file| load_arrow_metadata(file))
        .collect::<Result<Vec<_>>>()?;

      Ok(Some(Arc::new(ParquetInputSource {
        location: ParquetInputLocation::Local {
          input_path: options.location.clone(),
        },
        source_location: options.location.clone(),
        metadata,
      })))
    })
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
            .map(|rg| rg.num_rows() as u64)
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
      ParquetInputLocation::Local { input_path, .. } => {
        let input_path = input_path.clone();
        Box::pin(async move {
          let mut df = read_parquet_df(&input_path).await?;
          if !row_range.is_full() {
            df = df.limit(row_range.start, row_range.num)?;
          }

          let stream = execute_stream(df).await?;
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
      ParquetInputLocation::Local { input_path, .. } => {
        let input_path = input_path.clone();
        Box::pin(async move {
          let mut df = ctx.read_parquet(&input_path, Default::default()).await?;
          if !row_range.is_full() {
            df = df.limit(row_range.start, row_range.num)?;
          }
          Ok(df)
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
          let mut df = ctx.read_parquet(&input_url, Default::default()).await?;
          if !row_range.is_full() {
            df = df.limit(row_range.start, row_range.num)?;
          }
          Ok(df)
        })
      }
    }
  }
}

/// Open one HTTP Parquet object and load its footer metadata with range requests.
async fn open_http_parquet(location: &str) -> Result<Arc<dyn InputSource>> {
  if !location
    .split('?')
    .next()
    .is_some_and(|path| path.ends_with(".parquet"))
  {
    return Err(anyhow::anyhow!(
      "HTTP input must point directly to a .parquet file: {location}"
    ));
  }
  let parsed = Url::parse(location).with_context(|| format!("parse input URL: {location}"))?;
  let store_url = Url::parse(&format!(
    "{}://{}",
    parsed.scheme(),
    parsed
      .host_str()
      .context("HTTP input URL must include a host")?
  ))?;
  let object_path = ObjectPath::parse(parsed.path().trim_start_matches('/'))?;
  let store =
    Arc::new(HttpBuilder::new().with_url(store_url.as_str()).build()?) as Arc<dyn ObjectStore>;
  let object_meta = store
    .head(&object_path)
    .await
    .with_context(|| format!("read HTTP parquet metadata: {location}"))?;
  let mut reader = ParquetObjectReader::new(Arc::clone(&store), object_path.clone())
    .with_file_size(object_meta.size);
  let metadata = ArrowReaderMetadata::load_async(&mut reader, ArrowReaderOptions::new())
    .await
    .with_context(|| format!("read parquet footer: {location}"))?;

  Ok(Arc::new(ParquetInputSource {
    location: ParquetInputLocation::Http {
      input_url: location.to_string(),
      store_url,
      store,
      object_path,
      object_meta,
    },
    source_location: location.to_string(),
    metadata: vec![metadata],
  }))
}

/// Discover a single Parquet file or a sorted directory of Parquet files.
///
/// Returns `None` for unsupported paths so another input provider can attempt them.
fn discover_parquet_files(input: &Path) -> Result<Option<Vec<PathBuf>>> {
  if input.is_file() {
    return Ok(
      input
        .extension()
        .is_some_and(|ext| ext == "parquet")
        .then(|| vec![input.to_path_buf()]),
    );
  }

  if input.is_dir() {
    let mut files = Vec::new();
    for entry in fs::read_dir(input)? {
      let entry = entry?;
      let path = entry.path();
      if path.is_file() && path.extension().is_some_and(|ext| ext == "parquet") {
        files.push(path);
      }
    }
    if files.is_empty() {
      return Err(anyhow::anyhow!(
        "no parquet files found at: {}",
        input.display()
      ));
    }
    files.sort();
    return Ok(Some(files));
  }

  Ok(None)
}

/// Load Arrow and Parquet metadata from one local file footer.
fn load_arrow_metadata(file: &Path) -> Result<ArrowReaderMetadata> {
  ArrowReaderMetadata::load(
    &fs::File::open(file).with_context(|| format!("open parquet file: {}", file.display()))?,
    ArrowReaderOptions::new(),
  )
  .with_context(|| format!("read arrow metadata: {}", file.display()))
}

/// Parse and require consistent GeoParquet metadata across all discovered files.
fn load_geo_metadata(metadata_items: &[ArrowReaderMetadata]) -> Result<Option<GeoParquetMetadata>> {
  let mut geo_meta: Option<GeoParquetMetadata> = None;
  let mut saw_geo = false;
  let mut saw_missing_geo = false;

  for metadata in metadata_items {
    match parse_geo_metadata(&metadata).context("parse geoparquet metadata")? {
      Some(file_geo_meta) => {
        saw_geo = true;
        if let Some(existing) = geo_meta.as_mut() {
          existing.try_update(&file_geo_meta)?;
        } else {
          geo_meta = Some(file_geo_meta);
        }
      }
      None => saw_missing_geo = true,
    }
  }

  if saw_geo && saw_missing_geo {
    return Err(anyhow::anyhow!(
      "inconsistent geoparquet metadata across input files"
    ));
  }

  Ok(geo_meta)
}

/// Decode the GeoParquet `geo` key from one Parquet footer.
fn parse_geo_metadata(metadata: &ArrowReaderMetadata) -> Result<Option<GeoParquetMetadata>> {
  let Some(geo_value) = metadata
    .metadata()
    .file_metadata()
    .key_value_metadata()
    .and_then(|items| items.iter().find(|item| item.key == "geo"))
    .and_then(|item| item.value.as_ref())
  else {
    return Ok(None);
  };

  let mut json: Value = serde_json::from_str(geo_value).context("decode geo metadata json")?;
  sanitize_geo_metadata_json(&mut json);
  serde_json::from_value(json)
    .context("deserialize geo metadata")
    .map(Some)
}

/// Remove non-semantic metadata differences before comparing file-level GeoParquet JSON.
fn sanitize_geo_metadata_json(json: &mut Value) {
  let Some(columns) = json.get_mut("columns").and_then(Value::as_object_mut) else {
    return;
  };

  for column_meta in columns.values_mut() {
    let Some(object) = column_meta.as_object_mut() else {
      continue;
    };

    let remove_bbox = object
      .get("bbox")
      .and_then(Value::as_array)
      .is_some_and(|bbox| bbox.iter().any(Value::is_null));
    if remove_bbox {
      object.remove("bbox");
    }
  }
}

/// Convert GeoParquet metadata into the format-neutral source geometry model.
fn build_source_geometry_metadata(
  geo_meta: &GeoParquetMetadata,
) -> Result<Option<SourceGeometryMetadata>> {
  let Some(column_meta) = geo_meta.columns.get(&geo_meta.primary_column) else {
    return Ok(None);
  };
  if column_meta.encoding != GeoParquetColumnEncoding::WKB {
    return Ok(None);
  }

  let geometry_types = column_meta
    .geometry_types
    .iter()
    .map(|geometry_type| map_geo_geometry_type(geometry_type.geometry_type()))
    .collect();

  Ok(Some(SourceGeometryMetadata {
    column: geo_meta.primary_column.clone(),
    encoding: GeometryEncoding::Wkb,
    geometry_types,
    bbox: bbox_to_extent(column_meta.bbox.as_deref()),
    projjson: column_meta.crs.clone(),
    has_z: has_dimension_suffix(column_meta, "Z"),
    has_m: has_dimension_suffix(column_meta, "M"),
  }))
}

fn map_geo_geometry_type(geometry_type: GeoParquetGeometryType) -> GeometryKind {
  match geometry_type {
    GeoParquetGeometryType::Point => GeometryKind::Point,
    GeoParquetGeometryType::LineString => GeometryKind::LineString,
    GeoParquetGeometryType::MultiPoint => GeometryKind::MultiPoint,
    GeoParquetGeometryType::MultiLineString => GeometryKind::MultiLineString,
    GeoParquetGeometryType::Polygon => GeometryKind::Polygon,
    GeoParquetGeometryType::MultiPolygon => GeometryKind::MultiPolygon,
    GeoParquetGeometryType::GeometryCollection => GeometryKind::GeometryCollection,
  }
}

fn bbox_to_extent(bbox: Option<&[f64]>) -> Option<Extent2D> {
  let bbox = bbox?;
  if bbox.len() < 4 {
    return None;
  }
  Some(Extent2D {
    xmin: bbox[0],
    ymin: bbox[1],
    xmax: bbox[bbox.len() - 2],
    ymax: bbox[bbox.len() - 1],
  })
}

fn has_dimension_suffix(
  column_meta: &geoparquet::metadata::GeoParquetColumnMetadata,
  dimension: &str,
) -> bool {
  column_meta.geometry_types.iter().any(|geometry_type| {
    let value = geometry_type.to_string();
    match dimension {
      "Z" => value.ends_with(" Z") || value.ends_with(" ZM"),
      "M" => value.ends_with(" M") || value.ends_with(" ZM"),
      _ => false,
    }
  })
}

/// Preserve non-reserved key-value metadata exactly once across a file set.
fn passthrough_metadata(metadata_items: &[ArrowReaderMetadata]) -> Vec<KeyValue> {
  let mut seen = BTreeSet::new();
  let mut out = Vec::new();
  for metadata in metadata_items {
    if let Some(kv_metadata) = metadata.metadata().file_metadata().key_value_metadata() {
      for kv in kv_metadata {
        if kv.key == "geo" || kv.key == "geodisplay" || kv.key == "ARROW:schema" {
          continue;
        }
        let identity = (kv.key.clone(), kv.value.clone());
        if seen.insert(identity) {
          out.push(kv.clone());
        }
      }
    }
  }
  out
}

/// Collect all file metadata needed by callers, including reserved keys.
fn file_metadata(metadata_items: &[ArrowReaderMetadata]) -> Vec<KeyValue> {
  let mut seen = BTreeSet::new();
  let mut out = Vec::new();
  for metadata in metadata_items {
    if let Some(kv_metadata) = metadata.metadata().file_metadata().key_value_metadata() {
      for kv in kv_metadata {
        if kv.key == "ARROW:schema" {
          continue;
        }
        let identity = (kv.key.clone(), kv.value.clone());
        if seen.insert(identity) {
          out.push(kv.clone());
        }
      }
    }
  }
  out
}
