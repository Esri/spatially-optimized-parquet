use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use arrow_array::RecordBatch;
use arrow_schema::SchemaRef;
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;
use futures_util::Stream;
use futures_util::future::BoxFuture;
use futures_util::stream;
use gdal::spatial_ref::SpatialRef;
use parquet::file::metadata::KeyValue;
use tempfile::TempDir;
use tokio::runtime::Runtime;

use crate::geometry::{Extent2D, GeometryEncoding, GeometryKind, GeometrySpec};
use crate::input::{
  InputOpenOptions, InputSource, RowRange, SourceDatasetMetadata, SourceFormat,
  SourceGeometryMetadata, open_input,
};

use super::resolve_source;

use crate::test_support::{
  geoparquet_kv, sample_batch_with_geometry, sample_schema_with_geometry, wkb_point, write_parquet,
};

type InputBatchStream = Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send + 'static>>;

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

fn open_parquet_input(path: &Path) -> Arc<dyn InputSource> {
  runtime()
    .block_on(open_input(
      SourceFormat::Parquet,
      &InputOpenOptions::new(path.to_string_lossy().into_owned(), None),
    ))
    .unwrap()
}

struct MetadataInputSource {
  schema: SchemaRef,
  metadata: SourceDatasetMetadata,
  read_batch_calls: Arc<AtomicUsize>,
}

impl InputSource for MetadataInputSource {
  fn format_name(&self) -> &'static str {
    "metadata-test"
  }

  fn schema(&self) -> Result<SchemaRef> {
    Ok(self.schema.clone())
  }

  fn total_rows(&self) -> Result<u64> {
    Ok(3)
  }

  fn inferred_geometry_spec(&self) -> Result<Option<GeometrySpec>> {
    Ok(Some(GeometrySpec {
      column: "geometry".into(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: Some(GeometryKind::Point),
    }))
  }

  fn source_metadata(&self) -> Result<SourceDatasetMetadata> {
    Ok(self.metadata.clone())
  }

  fn read_batches(&self, _row_range: RowRange) -> BoxFuture<'_, Result<InputBatchStream>> {
    self.read_batch_calls.fetch_add(1, Ordering::SeqCst);
    Box::pin(async { Ok(Box::pin(stream::empty::<Result<RecordBatch>>()) as InputBatchStream) })
  }

  fn to_dataframe<'a>(
    &'a self,
    _ctx: &'a SessionContext,
    _row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>> {
    Box::pin(async { panic!("source context should not create a DataFrame") })
  }
}

fn epsg_projjson(wkid: u32) -> serde_json::Value {
  serde_json::from_str(&SpatialRef::from_epsg(wkid).unwrap().to_projjson().unwrap()).unwrap()
}

fn source_dataframe(input: &dyn InputSource) -> DataFrame {
  let context = SessionContext::new();
  runtime()
    .block_on(input.to_dataframe(&context, RowRange::default()))
    .unwrap()
}

fn empty_dataframe(schema: SchemaRef) -> DataFrame {
  SessionContext::new()
    .read_batch(RecordBatch::new_empty(schema))
    .unwrap()
}

#[test]
fn inferred_geometry_spec_reads_geoparquet_primary_column() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![Some(wkb_point(0.0, 0.0)), None, None]);
  let kv = vec![geoparquet_kv("geometry", &["Point"])];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let input = open_parquet_input(&path);
  let spec = input.inferred_geometry_spec().unwrap().unwrap();
  assert_eq!(spec.column, "geometry");
  assert_eq!(spec.geometry_kind, Some(GeometryKind::Point));
}

#[test]
fn source_metadata_collects_passthrough_kv_without_geo() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![None, None, None]);
  let kv = vec![
    geoparquet_kv("geometry", &["Point"]),
    KeyValue::new("source".to_string(), Some("census".to_string())),
  ];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let input = open_parquet_input(&path);
  let metadata = input.source_metadata().unwrap();
  let keys: HashMap<String, String> = metadata
    .passthrough_kv
    .into_iter()
    .filter_map(|kv| kv.value.map(|value| (kv.key, value)))
    .collect();
  assert_eq!(keys.get("source").map(String::as_str), Some("census"));
  assert!(!keys.contains_key("geo"));
}

#[test]
fn source_metadata_returns_none_when_geo_metadata_missing() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![Some(wkb_point(0.0, 0.0)), None, None]);
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[],
  );

  let input = open_parquet_input(&path);
  assert!(input.inferred_geometry_spec().unwrap().is_none());
  assert!(input.source_metadata().unwrap().geometry.is_none());
}

#[test]
fn source_metadata_tolerates_null_bbox_metadata() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![Some(wkb_point(0.0, 0.0)), None, None]);
  let kv = vec![KeyValue::new(
        "geo".to_string(),
        Some(
            "{\"version\":\"1.1.0\",\"primary_column\":\"geometry\",\"columns\":{\"geometry\":{\"encoding\":\"WKB\",\"geometry_types\":[\"Point\"],\"bbox\":[null,null,null,null]}}}".to_string(),
        ),
    )];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let input = open_parquet_input(&path);
  let metadata = input.source_metadata().unwrap();
  let geometry = metadata.geometry.unwrap();
  assert_eq!(geometry.column, "geometry");
  assert!(geometry.bbox.is_none());
  assert_eq!(geometry.geometry_types, vec![GeometryKind::Point]);
}

#[test]
fn resolved_source_retains_crs_and_extent_in_source_metadata() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![
    Some(wkb_point(-10.0, 5.0)),
    Some(wkb_point(20.0, 30.0)),
    None,
  ]);
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  let input = open_parquet_input(&path);
  let context = runtime()
    .block_on(resolve_source(
      input.as_ref(),
      source_dataframe(input.as_ref()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
    ))
    .unwrap();
  let source_geometry = context.source_metadata.geometry.unwrap();

  assert_eq!(source_geometry.bbox, Some(context.source_extent));
  assert_eq!(
    source_geometry.projjson,
    context.source_spatial_reference.projjson
  );
}

#[test]
fn resolved_source_uses_complete_metadata_without_scanning_batches() {
  let read_batch_calls = Arc::new(AtomicUsize::new(0));
  let expected_extent = Extent2D {
    xmin: -10.0,
    ymin: 5.0,
    xmax: 20.0,
    ymax: 30.0,
  };
  let input = MetadataInputSource {
    schema: sample_schema_with_geometry(),
    metadata: SourceDatasetMetadata {
      geometry: Some(SourceGeometryMetadata {
        column: "geometry".into(),
        encoding: GeometryEncoding::Wkb,
        geometry_types: vec![GeometryKind::Point],
        bbox: Some(expected_extent),
        projjson: Some(epsg_projjson(4326)),
        has_z: false,
        has_m: false,
      }),
      passthrough_kv: Vec::new(),
    },
    read_batch_calls: read_batch_calls.clone(),
  };

  let context = runtime()
    .block_on(resolve_source(
      &input,
      empty_dataframe(input.schema().unwrap()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
    ))
    .unwrap();

  assert_eq!(context.geometry_types, vec![GeometryKind::Point]);
  assert_eq!(context.source_extent, expected_extent);
  assert_eq!(read_batch_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn resolved_source_prefers_top_level_crs_authority_code() {
  let mut projjson = epsg_projjson(4269);
  projjson["datum"]["id"] = serde_json::json!({
    "authority": "EPSG",
    "code": 6269
  });
  let input = MetadataInputSource {
    schema: sample_schema_with_geometry(),
    metadata: SourceDatasetMetadata {
      geometry: Some(SourceGeometryMetadata {
        column: "geometry".into(),
        encoding: GeometryEncoding::Wkb,
        geometry_types: vec![GeometryKind::Point],
        bbox: Some(Extent2D {
          xmin: -10.0,
          ymin: 5.0,
          xmax: 20.0,
          ymax: 30.0,
        }),
        projjson: Some(projjson),
        has_z: false,
        has_m: false,
      }),
      passthrough_kv: Vec::new(),
    },
    read_batch_calls: Arc::new(AtomicUsize::new(0)),
  };

  let context = runtime()
    .block_on(resolve_source(
      &input,
      empty_dataframe(input.schema().unwrap()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
    ))
    .unwrap();

  assert_eq!(context.source_spatial_reference.wkid, Some(4269));
}

#[test]
fn resolved_source_preserves_projjson_for_unknown_crs_authority() {
  let mut projjson = epsg_projjson(4326);
  projjson["id"] = serde_json::json!({
    "authority": "IGNF",
    "code": 1234
  });
  let input = MetadataInputSource {
    schema: sample_schema_with_geometry(),
    metadata: SourceDatasetMetadata {
      geometry: Some(SourceGeometryMetadata {
        column: "geometry".into(),
        encoding: GeometryEncoding::Wkb,
        geometry_types: vec![GeometryKind::Point],
        bbox: Some(Extent2D {
          xmin: -10.0,
          ymin: 5.0,
          xmax: 20.0,
          ymax: 30.0,
        }),
        projjson: Some(projjson.clone()),
        has_z: false,
        has_m: false,
      }),
      passthrough_kv: Vec::new(),
    },
    read_batch_calls: Arc::new(AtomicUsize::new(0)),
  };

  let context = runtime()
    .block_on(resolve_source(
      &input,
      empty_dataframe(input.schema().unwrap()),
      input.schema().unwrap().as_ref(),
      None,
      None,
      RowRange::default(),
    ))
    .unwrap();

  assert_eq!(context.source_spatial_reference.wkid, None);
  assert_eq!(context.source_spatial_reference.projjson, Some(projjson));
}
