use std::collections::HashMap;

use parquet::file::metadata::KeyValue;
use spatial::input::RowRange;
use spatial::output::geoparquet::resolve_source_context;
use tempfile::TempDir;

mod common;
use common::{
  geoparquet_kv, open_parquet_input, sample_batch_with_geometry, sample_schema_with_geometry,
  wkb_point, write_parquet,
};

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
  assert_eq!(
    spec.geometry_kind,
    Some(spatial::geometry::GeometryKind::Point)
  );
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
  assert_eq!(
    geometry.geometry_types,
    vec![spatial::geometry::GeometryKind::Point]
  );
}

#[test]
fn source_context_retains_resolved_crs_and_extent_in_source_metadata() {
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
  let context = common::runtime()
    .block_on(resolve_source_context(
      input.as_ref(),
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
