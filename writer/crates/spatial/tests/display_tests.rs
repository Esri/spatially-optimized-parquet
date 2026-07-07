use std::sync::Arc;

use arrow_array::{
  Array, BinaryArray, Float64Array, RecordBatch, StringArray, StructArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use geo_types::{Geometry, Point, polygon};
use spatial::analysis::{DisplayGeometryType, DisplayJobAnalysis, Extent2D, GeometryFamily};
use spatial::display::{
  BOUNDS_COLUMN, DISPLAY_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN,
  TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_XZ_CODE_COLUMN, TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN,
  XZ_CODE_COLUMN, append_display_columns, build_output_schema, finalize_output_batch, sort_batches,
};
use spatial::geometry::{GeometryEncoding, GeometryKind, GeometrySpec};
use spatial::multiscale::{DISPLAY_OUTPUT_WKID, create_geometry_encodings};
use wkb::writer::WriteOptions;

fn write_wkb(geometry: &Geometry<f64>) -> Vec<u8> {
  let mut buffer = Vec::new();
  wkb::writer::write_geometry(&mut buffer, geometry, &WriteOptions::default()).unwrap();
  buffer
}

#[test]
fn point_display_columns_and_sort_are_generated() {
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_a = write_wkb(&Geometry::Point(Point::new(8.0, 8.0)));
  let point_b = write_wkb(&Geometry::Point(Point::new(1.0, 1.0)));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_a.as_slice()),
        Some(point_b.as_slice()),
      ])),
    ],
  )
  .unwrap();

  let analysis = DisplayJobAnalysis {
    geometry_spec: GeometrySpec {
      column: "geometry".to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: Some(GeometryKind::Point),
    },
    geometry_type: DisplayGeometryType::Point,
    geometry_family: GeometryFamily::Point,
    full_extent: Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 10.0,
      ymax: 10.0,
    },
    spatial_reference: Default::default(),
    has_z: false,
    has_m: false,
  };

  let output_schema = build_output_schema(&schema, &analysis, &[]);
  let transformed = append_display_columns(&batch, &output_schema, &analysis, &[]).unwrap();
  let sorted = sort_batches(&output_schema, &analysis, &[transformed], |_| {})
    .unwrap()
    .unwrap();

  let names = sorted
    .column_by_name("name")
    .unwrap()
    .as_any()
    .downcast_ref::<StringArray>()
    .unwrap();
  let z_codes = sorted
    .column_by_name(POINT_Z_CODE_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<UInt64Array>()
    .unwrap();
  let xs = sorted
    .column_by_name(POINT_X_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();
  let ys = sorted
    .column_by_name(POINT_Y_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();

  assert_eq!(names.value(0), "early");
  assert!(z_codes.value(0) <= z_codes.value(1));
  assert_eq!(xs.value(0), 1.0);
  assert_eq!(ys.value(0), 1.0);
}

#[test]
fn point_null_geometry_emits_nan_coordinates() {
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    true,
  )]));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![None::<&[u8]>]))],
  )
  .unwrap();

  let analysis = DisplayJobAnalysis {
    geometry_spec: GeometrySpec {
      column: "geometry".to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: Some(GeometryKind::Point),
    },
    geometry_type: DisplayGeometryType::Point,
    geometry_family: GeometryFamily::Point,
    full_extent: Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 10.0,
      ymax: 10.0,
    },
    spatial_reference: Default::default(),
    has_z: false,
    has_m: false,
  };

  let output_schema = build_output_schema(&schema, &analysis, &[]);
  let transformed = append_display_columns(&batch, &output_schema, &analysis, &[]).unwrap();
  let xs = transformed
    .column_by_name(POINT_X_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();
  let ys = transformed
    .column_by_name(POINT_Y_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();

  assert!(xs.value(0).is_nan());
  assert!(ys.value(0).is_nan());
}

#[test]
fn non_point_display_struct_and_sort_are_generated() {
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygon_a = write_wkb(&Geometry::Polygon(polygon![
      (x: 7.0, y: 7.0),
      (x: 9.0, y: 7.0),
      (x: 9.0, y: 9.0),
      (x: 7.0, y: 7.0),
  ]));
  let polygon_b = write_wkb(&Geometry::Polygon(polygon![
      (x: 0.0, y: 0.0),
      (x: 1.0, y: 0.0),
      (x: 1.0, y: 1.0),
      (x: 0.0, y: 0.0),
  ]));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["later", "earlier"])),
      Arc::new(BinaryArray::from(vec![
        Some(polygon_a.as_slice()),
        Some(polygon_b.as_slice()),
      ])),
    ],
  )
  .unwrap();

  let analysis = DisplayJobAnalysis {
    geometry_spec: GeometrySpec {
      column: "geometry".to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: Some(GeometryKind::Polygon),
    },
    geometry_type: DisplayGeometryType::Polygon,
    geometry_family: GeometryFamily::NonPoint,
    full_extent: Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 10.0,
      ymax: 10.0,
    },
    spatial_reference: Default::default(),
    has_z: false,
    has_m: false,
  };
  let encodings = create_geometry_encodings(DISPLAY_OUTPUT_WKID, analysis.geometry_type).unwrap();
  let output_schema = build_output_schema(&schema, &analysis, &encodings);
  let transformed = append_display_columns(&batch, &output_schema, &analysis, &encodings).unwrap();
  let sorted = sort_batches(&output_schema, &analysis, &[transformed], |_| {})
    .unwrap()
    .unwrap();

  let names = sorted
    .column_by_name("name")
    .unwrap()
    .as_any()
    .downcast_ref::<StringArray>()
    .unwrap();
  let display = sorted
    .column_by_name(DISPLAY_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let xz_codes = display
    .column_by_name(XZ_CODE_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<UInt64Array>()
    .unwrap();
  let bounds = display
    .column_by_name(BOUNDS_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();

  assert_eq!(names.value(0), "earlier");
  assert!(xz_codes.value(0) <= xz_codes.value(1));
  assert!(display.column_by_name(&encodings[0].column).is_some());
  assert_eq!(
    bounds.column_by_name("xmin").unwrap().data_type(),
    &DataType::Float64
  );
}

#[test]
fn non_point_null_geometry_writes_null_multiscale_columns() {
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    true,
  )]));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![None::<&[u8]>]))],
  )
  .unwrap();

  let analysis = DisplayJobAnalysis {
    geometry_spec: GeometrySpec {
      column: "geometry".to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: Some(GeometryKind::Polygon),
    },
    geometry_type: DisplayGeometryType::Polygon,
    geometry_family: GeometryFamily::NonPoint,
    full_extent: Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 10.0,
      ymax: 10.0,
    },
    spatial_reference: Default::default(),
    has_z: false,
    has_m: false,
  };

  let encodings = create_geometry_encodings(DISPLAY_OUTPUT_WKID, analysis.geometry_type).unwrap();
  let output_schema = build_output_schema(&schema, &analysis, &encodings);
  let transformed = append_display_columns(&batch, &output_schema, &analysis, &encodings).unwrap();
  let display = transformed
    .column_by_name(DISPLAY_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let xz_codes = display
    .column_by_name(XZ_CODE_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<UInt64Array>()
    .unwrap();
  let bounds = display
    .column_by_name(BOUNDS_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let level = display
    .column_by_name(&encodings[0].column)
    .unwrap()
    .as_any()
    .downcast_ref::<BinaryArray>()
    .unwrap();

  assert_eq!(xz_codes.value(0), 0);
  assert!(bounds.is_null(0));
  assert!(level.is_null(0));
}

#[test]
fn non_point_multiscale_levels_produce_different_payloads() {
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    true,
  )]));
  let polyline = write_wkb(&Geometry::LineString(geo_types::LineString::from(vec![
    (0.0, 0.0),
    (0.2, 0.2),
    (0.4, 0.4),
    (0.6, 0.6),
    (0.8, 0.8),
    (1.0, 1.0),
    (1.2, 1.2),
    (1.4, 1.4),
  ])));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(polyline.as_slice())]))],
  )
  .unwrap();

  let analysis = DisplayJobAnalysis {
    geometry_spec: GeometrySpec {
      column: "geometry".to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: Some(GeometryKind::LineString),
    },
    geometry_type: DisplayGeometryType::Polyline,
    geometry_family: GeometryFamily::NonPoint,
    full_extent: Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 2.0,
      ymax: 2.0,
    },
    spatial_reference: Default::default(),
    has_z: false,
    has_m: false,
  };

  let encodings = create_geometry_encodings(DISPLAY_OUTPUT_WKID, analysis.geometry_type).unwrap();
  let output_schema = build_output_schema(&schema, &analysis, &encodings);
  let transformed = append_display_columns(&batch, &output_schema, &analysis, &encodings).unwrap();
  let display = transformed
    .column_by_name(DISPLAY_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let coarse = display
    .column_by_name(&encodings[0].column)
    .unwrap()
    .as_any()
    .downcast_ref::<BinaryArray>()
    .unwrap();
  let fine = display
    .column_by_name(&encodings[1].column)
    .unwrap()
    .as_any()
    .downcast_ref::<BinaryArray>()
    .unwrap();

  assert_ne!(coarse.value(0), fine.value(0));
}

#[test]
fn finalize_non_point_batch_builds_geodisplay_from_parallel_encoder() {
  let source_schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let batch_size = 600;
  let polygon = write_wkb(&Geometry::Polygon(polygon![
      (x: 0.0, y: 0.0),
      (x: 1.0, y: 0.0),
      (x: 1.0, y: 1.0),
      (x: 0.0, y: 0.0),
  ]));

  let analysis = DisplayJobAnalysis {
    geometry_spec: GeometrySpec {
      column: "geometry".to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: Some(GeometryKind::Polygon),
    },
    geometry_type: DisplayGeometryType::Polygon,
    geometry_family: GeometryFamily::NonPoint,
    full_extent: Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 2.0,
      ymax: 2.0,
    },
    spatial_reference: Default::default(),
    has_z: false,
    has_m: false,
  };
  let encodings = create_geometry_encodings(DISPLAY_OUTPUT_WKID, analysis.geometry_type).unwrap();
  let output_schema = build_output_schema(&source_schema, &analysis, &encodings);
  let temp_schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
    Field::new(TEMP_XZ_CODE_COLUMN, DataType::UInt64, false),
    Field::new(TEMP_XMIN_COLUMN, DataType::Float64, true),
    Field::new(TEMP_YMIN_COLUMN, DataType::Float64, true),
    Field::new(TEMP_XMAX_COLUMN, DataType::Float64, true),
    Field::new(TEMP_YMAX_COLUMN, DataType::Float64, true),
  ]));
  let batch = RecordBatch::try_new(
    temp_schema,
    vec![
      Arc::new(StringArray::from(
        (0..batch_size)
          .map(|index| format!("row-{index}"))
          .collect::<Vec<_>>(),
      )),
      Arc::new(BinaryArray::from(vec![
        Some(polygon.as_slice());
        batch_size
      ])),
      Arc::new(UInt64Array::from(vec![42_u64; batch_size])),
      Arc::new(Float64Array::from(vec![0.0; batch_size])),
      Arc::new(Float64Array::from(vec![0.0; batch_size])),
      Arc::new(Float64Array::from(vec![1.0; batch_size])),
      Arc::new(Float64Array::from(vec![1.0; batch_size])),
    ],
  )
  .unwrap();

  let finalized = finalize_output_batch(&batch, &output_schema, &analysis, &encodings).unwrap();
  let display = finalized
    .column_by_name(DISPLAY_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let bounds = display
    .column_by_name(BOUNDS_COLUMN)
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let xmin = bounds
    .column_by_name("xmin")
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();
  let xmax = bounds
    .column_by_name("xmax")
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();
  let level = display
    .column_by_name(&encodings[0].column)
    .unwrap()
    .as_any()
    .downcast_ref::<BinaryArray>()
    .unwrap();

  assert_eq!(finalized.num_rows(), batch_size);
  assert!(finalized.column_by_name(TEMP_XZ_CODE_COLUMN).is_none());
  assert_eq!(xmin.value(0), 0.0);
  assert_eq!(xmax.value(batch_size - 1), 1.0);
  assert!(!level.is_null(0));
  assert!(!level.is_null(batch_size - 1));
}
