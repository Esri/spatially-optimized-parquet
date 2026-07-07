use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use arrow_array::{BinaryArray, Int32Array, RecordBatch};
use engine::{DataFrame, SessionContext};
use futures_util::future::BoxFuture;
use futures_util::stream;
use geo::polygon;
use parquet::file::metadata::KeyValue;
use spatial::analysis::{
  DisplayGeometryType, Extent2D, GeometryFamily, analyze_display_job,
  analyze_display_job_with_progress,
};
use spatial::geometry::{GeometryEncoding, GeometryKind, GeometrySpec};
use spatial::input::{InputBatchStream, InputSource, RowRange};
use spatial::metadata::source::{SourceDatasetMetadata, SourceGeometryMetadata};
use tempfile::TempDir;

mod common;
use common::{
  geoparquet_kv, open_parquet_input, runtime, sample_batch_with_geometry,
  sample_schema_with_geometry, wkb_point, write_parquet,
};

fn wkb_polygon(xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> Vec<u8> {
  let geom = geo::Geometry::Polygon(geo::polygon![
      (x: xmin, y: ymin),
      (x: xmax, y: ymin),
      (x: xmax, y: ymax),
      (x: xmin, y: ymax),
      (x: xmin, y: ymin),
  ]);
  let mut buf = Vec::new();
  wkb::writer::write_geometry(&mut buf, &geom, &Default::default()).unwrap();
  buf
}

struct TestInputSource {
  path: PathBuf,
  total_rows: u64,
  batch: RecordBatch,
  read_batch_calls: Arc<AtomicUsize>,
}

impl InputSource for TestInputSource {
  fn format_name(&self) -> &'static str {
    "test"
  }

  fn source_location(&self) -> &str {
    self.path.to_str().unwrap()
  }

  fn schema(&self) -> Result<arrow_schema::SchemaRef> {
    Ok(sample_schema_with_geometry())
  }

  fn total_rows(&self) -> Result<u64> {
    Ok(self.total_rows)
  }

  fn inferred_geometry_spec(&self) -> Result<Option<GeometrySpec>> {
    Ok(None)
  }

  fn source_metadata(&self) -> Result<SourceDatasetMetadata> {
    Ok(SourceDatasetMetadata::default())
  }

  fn read_batches(&self, _row_range: RowRange) -> BoxFuture<'_, Result<InputBatchStream>> {
    self.read_batch_calls.fetch_add(1, Ordering::SeqCst);
    let batch = self.batch.clone();
    Box::pin(async move { Ok(Box::pin(stream::iter(vec![Ok(batch)])) as InputBatchStream) })
  }

  fn to_dataframe<'a>(
    &'a self,
    _ctx: &'a SessionContext,
    _row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>> {
    Box::pin(async { panic!("to_dataframe should not be called during analysis") })
  }
}

fn source_metadata_with_bbox(
  geometry_types: Vec<GeometryKind>,
  bbox: Extent2D,
) -> SourceDatasetMetadata {
  SourceDatasetMetadata {
    geometry: Some(SourceGeometryMetadata {
      column: "geometry".into(),
      encoding: GeometryEncoding::Wkb,
      geometry_types,
      bbox: Some(bbox),
      projjson: None,
      has_z: false,
      has_m: false,
    }),
    passthrough_kv: Vec::new(),
  }
}

#[test]
fn analyze_display_job_detects_point_dataset() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("points.parquet");
  let schema = sample_schema_with_geometry();
  let batch = common::sample_batch_with_geometry(vec![
    Some(wkb_point(-10.0, 5.0)),
    Some(wkb_point(4.0, 12.0)),
    None,
  ]);
  let kv = vec![geoparquet_kv("geometry", &["Point"])];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let spec = GeometrySpec {
    column: "geometry".into(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: None,
  };
  let input = open_parquet_input(&path);
  let source_metadata = input.source_metadata().unwrap();
  let analysis = runtime()
    .block_on(analyze_display_job(input.as_ref(), &spec, &source_metadata))
    .unwrap();
  assert_eq!(analysis.geometry_family, GeometryFamily::Point);
  assert_eq!(analysis.geometry_type, DisplayGeometryType::Point);
  assert_eq!(analysis.full_extent.xmin, -10.0);
  assert_eq!(analysis.full_extent.ymin, 5.0);
  assert_eq!(analysis.full_extent.xmax, 4.0);
  assert_eq!(analysis.full_extent.ymax, 12.0);
}

#[test]
fn analyze_display_job_detects_polygon_dataset() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("polygons.parquet");
  let schema = sample_schema_with_geometry();
  let batch = common::sample_batch_with_geometry(vec![
    Some(wkb_polygon(0.0, 0.0, 3.0, 4.0)),
    Some(wkb_polygon(-5.0, 2.0, -1.0, 7.0)),
    None,
  ]);
  let kv = vec![geoparquet_kv("geometry", &["Polygon"])];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let spec = GeometrySpec {
    column: "geometry".into(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: None,
  };
  let input = open_parquet_input(&path);
  let source_metadata = input.source_metadata().unwrap();
  let analysis = runtime()
    .block_on(analyze_display_job(input.as_ref(), &spec, &source_metadata))
    .unwrap();
  assert_eq!(analysis.geometry_family, GeometryFamily::NonPoint);
  assert_eq!(analysis.geometry_type, DisplayGeometryType::Polygon);
  assert_eq!(analysis.full_extent.xmin, -5.0);
  assert_eq!(analysis.full_extent.ymin, 0.0);
  assert_eq!(analysis.full_extent.xmax, 3.0);
  assert_eq!(analysis.full_extent.ymax, 7.0);
}

#[test]
fn analyze_display_job_rejects_mixed_display_geometry_types() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("mixed.parquet");
  let schema = sample_schema_with_geometry();
  let batch = common::sample_batch_with_geometry(vec![
    Some(wkb_point(0.0, 0.0)),
    Some(wkb_polygon(0.0, 0.0, 1.0, 1.0)),
    None,
  ]);
  let kv = vec![geoparquet_kv("geometry", &["Point", "Polygon"])];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let spec = GeometrySpec {
    column: "geometry".into(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: None,
  };
  let input = open_parquet_input(&path);
  let source_metadata = input.source_metadata().unwrap();
  let err = runtime()
    .block_on(analyze_display_job(input.as_ref(), &spec, &source_metadata))
    .unwrap_err();
  assert!(err.to_string().contains("mixed display geometry types"));
}

#[test]
fn analyze_display_job_prefers_top_level_crs_id_for_wkid() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("points.parquet");
  let schema = sample_schema_with_geometry();
  let batch = common::sample_batch_with_geometry(vec![
    Some(wkb_point(-10.0, 5.0)),
    Some(wkb_point(4.0, 12.0)),
    None,
  ]);
  let kv = vec![KeyValue::new(
        "geo".to_string(),
        Some(
            "{\"version\":\"1.1.0\",\"primary_column\":\"geometry\",\"columns\":{\"geometry\":{\"encoding\":\"WKB\",\"geometry_types\":[\"Point\"],\"crs\":{\"type\":\"GeographicCRS\",\"name\":\"NAD83\",\"datum\":{\"id\":{\"authority\":\"EPSG\",\"code\":6269}},\"transformation\":{\"id\":{\"authority\":\"EPSG\",\"code\":6296}},\"id\":{\"authority\":\"EPSG\",\"code\":4269}}}}}".to_string(),
        ),
    )];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let spec = GeometrySpec {
    column: "geometry".into(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: None,
  };
  let input = open_parquet_input(&path);
  let source_metadata = input.source_metadata().unwrap();
  let analysis = runtime()
    .block_on(analyze_display_job(input.as_ref(), &spec, &source_metadata))
    .unwrap();
  assert_eq!(analysis.spatial_reference.wkid, Some(4269));
}

#[test]
fn analyze_display_job_preserves_projjson_for_unknown_crs_authority() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("points.parquet");
  let schema = sample_schema_with_geometry();
  let batch = common::sample_batch_with_geometry(vec![
    Some(wkb_point(-10.0, 5.0)),
    Some(wkb_point(4.0, 12.0)),
    None,
  ]);
  let kv = vec![KeyValue::new(
        "geo".to_string(),
        Some(
            "{\"version\":\"1.1.0\",\"primary_column\":\"geometry\",\"columns\":{\"geometry\":{\"encoding\":\"WKB\",\"geometry_types\":[\"Point\"],\"crs\":{\"type\":\"GeographicCRS\",\"name\":\"Custom CRS\",\"id\":{\"authority\":\"IGNF\",\"code\":1234}}}}}".to_string(),
        ),
    )];
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &kv,
  );

  let spec = GeometrySpec {
    column: "geometry".into(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: None,
  };
  let input = open_parquet_input(&path);
  let source_metadata = input.source_metadata().unwrap();
  let analysis = runtime()
    .block_on(analyze_display_job(input.as_ref(), &spec, &source_metadata))
    .unwrap();
  assert_eq!(analysis.spatial_reference.wkid, None);
  assert_eq!(
    analysis.spatial_reference.projjson.unwrap()["id"]["authority"],
    "IGNF"
  );
}

#[test]
fn analyze_display_job_skips_batch_scan_when_metadata_is_sufficient() {
  let read_batch_calls = Arc::new(AtomicUsize::new(0));
  let input = TestInputSource {
    path: PathBuf::from("metadata-fast-path"),
    total_rows: 3,
    batch: sample_batch_with_geometry(vec![
      Some(wkb_point(999.0, 999.0)),
      Some(wkb_point(1000.0, 1000.0)),
      None,
    ]),
    read_batch_calls: read_batch_calls.clone(),
  };
  let source_metadata = source_metadata_with_bbox(
    vec![GeometryKind::Point],
    Extent2D {
      xmin: -10.0,
      ymin: 5.0,
      xmax: 4.0,
      ymax: 12.0,
    },
  );
  let spec = GeometrySpec {
    column: "geometry".into(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: None,
  };
  let mut progress_rows = 0u64;
  let analysis = runtime()
    .block_on(analyze_display_job_with_progress(
      &input,
      &spec,
      &source_metadata,
      None,
      |rows| progress_rows += rows,
    ))
    .unwrap();

  assert_eq!(analysis.geometry_family, GeometryFamily::Point);
  assert_eq!(analysis.geometry_type, DisplayGeometryType::Point);
  assert_eq!(analysis.full_extent.xmin, -10.0);
  assert_eq!(analysis.full_extent.ymin, 5.0);
  assert_eq!(analysis.full_extent.xmax, 4.0);
  assert_eq!(analysis.full_extent.ymax, 12.0);
  assert_eq!(progress_rows, 3);
  assert_eq!(read_batch_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn analyze_display_job_limit_disables_metadata_fast_path() {
  let read_batch_calls = Arc::new(AtomicUsize::new(0));
  let input = TestInputSource {
    path: PathBuf::from("metadata-limit-fallback"),
    total_rows: 3,
    batch: sample_batch_with_geometry(vec![
      Some(wkb_point(-2.0, 1.0)),
      Some(wkb_polygon(0.0, 0.0, 1.0, 1.0)),
      None,
    ]),
    read_batch_calls: read_batch_calls.clone(),
  };
  let source_metadata = source_metadata_with_bbox(
    vec![GeometryKind::Point],
    Extent2D {
      xmin: -50.0,
      ymin: -50.0,
      xmax: 500.0,
      ymax: 500.0,
    },
  );
  let spec = GeometrySpec {
    column: "geometry".into(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: None,
  };
  let mut progress_rows = 0u64;
  let err = runtime()
    .block_on(analyze_display_job_with_progress(
      &input,
      &spec,
      &source_metadata,
      Some(2),
      |rows| progress_rows += rows,
    ))
    .unwrap_err();

  assert!(err.to_string().contains("mixed display geometry types"));
  assert_eq!(progress_rows, 1);
  assert_eq!(read_batch_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn analyze_display_job_reports_progress_within_large_single_batch() {
  let row_count = 1_000usize;
  let read_batch_calls = Arc::new(AtomicUsize::new(0));
  let ids = Int32Array::from_iter_values((0..row_count).map(|value| value as i32));
  let point = wkb_point(-2.0, 1.0);
  let geometry = BinaryArray::from(
    std::iter::repeat_with(|| Some(point.as_slice()))
      .take(row_count)
      .collect::<Vec<_>>(),
  );
  let input = TestInputSource {
    path: PathBuf::from("single-batch-progress"),
    total_rows: row_count as u64,
    batch: RecordBatch::try_new(
      sample_schema_with_geometry(),
      vec![Arc::new(ids), Arc::new(geometry)],
    )
    .unwrap(),
    read_batch_calls: read_batch_calls.clone(),
  };
  let spec = GeometrySpec {
    column: "geometry".into(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: None,
  };
  let mut progress_events = Vec::new();
  let analysis = runtime()
    .block_on(analyze_display_job_with_progress(
      &input,
      &spec,
      &SourceDatasetMetadata::default(),
      None,
      |rows| progress_events.push(rows),
    ))
    .unwrap();

  assert_eq!(analysis.geometry_family, GeometryFamily::Point);
  assert_eq!(progress_events.iter().sum::<u64>(), row_count as u64);
  assert!(progress_events.len() > 1);
  assert_eq!(read_batch_calls.load(Ordering::SeqCst), 1);
}
