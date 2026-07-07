use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::builder::{BinaryBuilder, Float64Builder, StructBuilder, UInt64Builder};
use arrow_array::{
  Array, BinaryArray, BinaryViewArray, Float64Array, LargeBinaryArray, RecordBatch, StructArray,
  UInt64Array,
};
use arrow_ord::sort::sort_to_indices;
use arrow_schema::{DataType, Field, Fields, Schema, SchemaRef};
use arrow_select::concat::concat_batches;
use arrow_select::take::take_record_batch;
use rayon::prelude::*;

use crate::analysis::{DisplayGeometryType, DisplayJobAnalysis, GeometryFamily};
use crate::codes::{DEFAULT_COORDINATE_PRECISION, extent_xz_code, point_z_code};
use crate::multiscale::GeometryEncoding;
use crate::pbf::{
  GeometryEncodeScratch, encode_flat_geometry_owned_with_scratch,
  encode_flat_geometry_with_scratch, encode_geometry, flat_geometry_payload_from_wkb,
  geometry_payload_from_wkb, point_xy_from_wkb,
};

pub const POINT_Z_CODE_COLUMN: &str = "zCode";
pub const POINT_X_COLUMN: &str = "x";
pub const POINT_Y_COLUMN: &str = "y";
pub const DISPLAY_COLUMN: &str = "geodisplay";
pub const XZ_CODE_COLUMN: &str = "xzCode";
pub const BOUNDS_COLUMN: &str = "bounds";
pub const COVERING_BBOX_COLUMN: &str = "bbox";
pub const TEMP_POINT_COORDS_COLUMN: &str = "__display_point_coords";
pub const TEMP_BOUNDS_COLUMN: &str = "__display_bounds";
pub const TEMP_REPROJECTED_GEOMETRY_COLUMN: &str = "__display_reprojected_geometry";
pub const TEMP_XZ_CODE_COLUMN: &str = "__display_xzcode";
pub const TEMP_XMIN_COLUMN: &str = "__display_xmin";
pub const TEMP_YMIN_COLUMN: &str = "__display_ymin";
pub const TEMP_XMAX_COLUMN: &str = "__display_xmax";
pub const TEMP_YMAX_COLUMN: &str = "__display_ymax";

const NON_POINT_PARALLEL_MIN_ROWS: usize = 512;
const NON_POINT_ENCODE_WINDOW_ROWS: usize = 1024;
const NON_POINT_ENCODE_CHUNK_ROWS: usize = 128;

pub fn build_output_schema(
  input_schema: &SchemaRef,
  analysis: &DisplayJobAnalysis,
  encodings: &[GeometryEncoding],
) -> SchemaRef {
  let mut fields = input_schema.fields().iter().cloned().collect::<Vec<_>>();
  match analysis.geometry_family {
    GeometryFamily::Point => {
      fields.push(Arc::new(Field::new(
        POINT_Z_CODE_COLUMN,
        DataType::UInt64,
        false,
      )));
      fields.push(Arc::new(Field::new(
        POINT_X_COLUMN,
        DataType::Float64,
        false,
      )));
      fields.push(Arc::new(Field::new(
        POINT_Y_COLUMN,
        DataType::Float64,
        false,
      )));
    }
    GeometryFamily::NonPoint => {
      let mut display_fields = vec![
        Arc::new(Field::new(XZ_CODE_COLUMN, DataType::UInt64, false)),
        Arc::new(Field::new(
          BOUNDS_COLUMN,
          DataType::Struct(Fields::from(vec![
            Arc::new(Field::new("xmin", DataType::Float64, true)),
            Arc::new(Field::new("ymin", DataType::Float64, true)),
            Arc::new(Field::new("xmax", DataType::Float64, true)),
            Arc::new(Field::new("ymax", DataType::Float64, true)),
          ])),
          true,
        )),
      ];

      for encoding in encodings {
        display_fields.push(Arc::new(Field::new(
          &encoding.column,
          DataType::Binary,
          true,
        )));
      }

      fields.push(Arc::new(Field::new(
        DISPLAY_COLUMN,
        DataType::Struct(Fields::from(display_fields)),
        false,
      )));
    }
  }

  Arc::new(Schema::new_with_metadata(
    fields,
    input_schema.metadata.clone(),
  ))
}

pub fn append_display_columns(
  batch: &RecordBatch,
  output_schema: &SchemaRef,
  analysis: &DisplayJobAnalysis,
  encodings: &[GeometryEncoding],
) -> Result<RecordBatch> {
  let mut columns = batch.columns().to_vec();
  match analysis.geometry_family {
    GeometryFamily::Point => append_point_columns(batch, &mut columns, analysis)?,
    GeometryFamily::NonPoint => append_non_point_columns(
      batch,
      &mut columns,
      &analysis.geometry_spec.column,
      analysis.full_extent,
      analysis.geometry_type,
      encodings,
    )?,
  }

  RecordBatch::try_new(output_schema.clone(), columns).context("build display record batch")
}

pub fn sort_batches(
  schema: &SchemaRef,
  analysis: &DisplayJobAnalysis,
  batches: &[RecordBatch],
  mut on_progress: impl FnMut(u64),
) -> Result<Option<RecordBatch>> {
  if batches.is_empty() {
    return Ok(None);
  }

  let merged = concat_batches(schema, batches.iter())?;
  on_progress(merged.num_rows() as u64);
  if merged.num_rows() == 0 {
    return Ok(Some(merged));
  }

  let sort_array: Arc<dyn Array> = match analysis.geometry_family {
    GeometryFamily::Point => merged
      .column_by_name(POINT_Z_CODE_COLUMN)
      .context("missing zCode column")?
      .clone(),
    GeometryFamily::NonPoint => {
      let struct_array = merged
        .column_by_name(DISPLAY_COLUMN)
        .context("missing geodisplay column")?
        .as_any()
        .downcast_ref::<StructArray>()
        .context("geodisplay is not a struct array")?;
      struct_array
        .column_by_name(XZ_CODE_COLUMN)
        .context("missing xzCode column")?
        .clone()
    }
  };

  let indices = sort_to_indices(sort_array.as_ref(), None, None)?;
  on_progress(merged.num_rows() as u64);
  let sorted = take_record_batch(&merged, &indices)?;
  on_progress(sorted.num_rows() as u64);
  Ok(Some(sorted))
}

pub fn finalize_output_batch(
  batch: &RecordBatch,
  output_schema: &SchemaRef,
  analysis: &DisplayJobAnalysis,
  encodings: &[GeometryEncoding],
) -> Result<RecordBatch> {
  match analysis.geometry_family {
    GeometryFamily::Point => finalize_point_batch(batch, output_schema),
    GeometryFamily::NonPoint => finalize_non_point_batch(batch, output_schema, analysis, encodings),
  }
}

fn finalize_point_batch(batch: &RecordBatch, output_schema: &SchemaRef) -> Result<RecordBatch> {
  if batch.schema() == *output_schema {
    return Ok(batch.clone());
  }
  Ok(RecordBatch::try_new(
    output_schema.clone(),
    batch.columns().to_vec(),
  )?)
}

fn finalize_non_point_batch(
  batch: &RecordBatch,
  output_schema: &SchemaRef,
  analysis: &DisplayJobAnalysis,
  encodings: &[GeometryEncoding],
) -> Result<RecordBatch> {
  let geometry_column = batch
    .column_by_name(&analysis.geometry_spec.column)
    .with_context(|| {
      format!(
        "missing geometry column '{}'",
        analysis.geometry_spec.column
      )
    })?;
  let xz_code_column = batch
    .column_by_name(TEMP_XZ_CODE_COLUMN)
    .context("missing temporary xzCode column")?
    .as_any()
    .downcast_ref::<UInt64Array>()
    .context("temporary xzCode column is not UInt64")?;
  let xmin_column = batch
    .column_by_name(TEMP_XMIN_COLUMN)
    .context("missing temporary xmin column")?
    .as_any()
    .downcast_ref::<Float64Array>()
    .context("temporary xmin column is not Float64")?;
  let ymin_column = batch
    .column_by_name(TEMP_YMIN_COLUMN)
    .context("missing temporary ymin column")?
    .as_any()
    .downcast_ref::<Float64Array>()
    .context("temporary ymin column is not Float64")?;
  let xmax_column = batch
    .column_by_name(TEMP_XMAX_COLUMN)
    .context("missing temporary xmax column")?
    .as_any()
    .downcast_ref::<Float64Array>()
    .context("temporary xmax column is not Float64")?;
  let ymax_column = batch
    .column_by_name(TEMP_YMAX_COLUMN)
    .context("missing temporary ymax column")?
    .as_any()
    .downcast_ref::<Float64Array>()
    .context("temporary ymax column is not Float64")?;
  let bounds_fields = Fields::from(vec![
    Arc::new(Field::new("xmin", DataType::Float64, true)),
    Arc::new(Field::new("ymin", DataType::Float64, true)),
    Arc::new(Field::new("xmax", DataType::Float64, true)),
    Arc::new(Field::new("ymax", DataType::Float64, true)),
  ]);
  let bounds = StructArray::try_new(
    bounds_fields.clone(),
    vec![
      Arc::new(xmin_column.clone()),
      Arc::new(ymin_column.clone()),
      Arc::new(xmax_column.clone()),
      Arc::new(ymax_column.clone()),
    ],
    xmin_column.nulls().cloned(),
  )?;
  let geometry_values = collect_geometry_values(geometry_column.as_ref())?;
  let estimated_builder_bytes = estimated_pbf_builder_bytes(&geometry_values);
  let mut pbf_builders = encodings
    .iter()
    .map(|_| BinaryBuilder::with_capacity(geometry_values.len(), estimated_builder_bytes))
    .collect::<Vec<_>>();
  append_non_point_payload_columns(
    &geometry_values,
    analysis.geometry_type,
    encodings,
    &mut pbf_builders,
  )?;

  let mut display_fields: Vec<Arc<Field>> = vec![
    Arc::new(Field::new(XZ_CODE_COLUMN, DataType::UInt64, false)),
    Arc::new(Field::new(
      BOUNDS_COLUMN,
      DataType::Struct(bounds_fields),
      true,
    )),
  ];
  let mut display_columns: Vec<Arc<dyn Array>> =
    vec![Arc::new(xz_code_column.clone()), Arc::new(bounds)];
  for (mut builder, encoding) in pbf_builders.into_iter().zip(encodings) {
    display_fields.push(Arc::new(Field::new(
      &encoding.column,
      DataType::Binary,
      true,
    )));
    display_columns.push(Arc::new(builder.finish()));
  }
  let geodisplay = Arc::new(StructArray::try_new(
    Fields::from(display_fields),
    display_columns,
    None,
  )?);

  let input_columns = output_schema.fields().len().saturating_sub(1);
  let mut columns = batch.columns()[..input_columns].to_vec();
  columns.push(geodisplay);
  Ok(RecordBatch::try_new(output_schema.clone(), columns)?)
}

fn append_point_columns(
  batch: &RecordBatch,
  columns: &mut Vec<Arc<dyn Array>>,
  analysis: &DisplayJobAnalysis,
) -> Result<()> {
  let geometry_column = batch
    .column_by_name(&analysis.geometry_spec.column)
    .with_context(|| {
      format!(
        "missing geometry column '{}'",
        analysis.geometry_spec.column
      )
    })?;
  let mut z_builder = UInt64Builder::with_capacity(geometry_column.len());
  let mut x_builder = Float64Builder::with_capacity(geometry_column.len());
  let mut y_builder = Float64Builder::with_capacity(geometry_column.len());

  for_each_geometry_value(geometry_column.as_ref(), |bytes| {
    if let Some(bytes) = bytes {
      let (x, y) = point_xy_from_wkb(bytes)?;
      x_builder.append_value(x);
      y_builder.append_value(y);
      z_builder.append_value(point_z_code(
        analysis.full_extent,
        x,
        y,
        DEFAULT_COORDINATE_PRECISION,
      ));
    } else {
      z_builder.append_value(0);
      x_builder.append_value(f64::NAN);
      y_builder.append_value(f64::NAN);
    }
    Ok(())
  })?;

  columns.push(Arc::new(z_builder.finish()));
  columns.push(Arc::new(x_builder.finish()));
  columns.push(Arc::new(y_builder.finish()));
  Ok(())
}

fn append_non_point_columns(
  batch: &RecordBatch,
  columns: &mut Vec<Arc<dyn Array>>,
  geometry_column: &str,
  full_extent: crate::analysis::Extent2D,
  geometry_type: DisplayGeometryType,
  encodings: &[GeometryEncoding],
) -> Result<()> {
  let geometry_column = batch
    .column_by_name(geometry_column)
    .context("missing geometry column")?;
  let mut code_builder = UInt64Builder::with_capacity(geometry_column.len());
  let bounds_fields = Fields::from(vec![
    Arc::new(Field::new("xmin", DataType::Float64, true)),
    Arc::new(Field::new("ymin", DataType::Float64, true)),
    Arc::new(Field::new("xmax", DataType::Float64, true)),
    Arc::new(Field::new("ymax", DataType::Float64, true)),
  ]);
  let mut bounds_builder = StructBuilder::new(
    bounds_fields.clone(),
    vec![
      Box::new(Float64Builder::with_capacity(geometry_column.len())),
      Box::new(Float64Builder::with_capacity(geometry_column.len())),
      Box::new(Float64Builder::with_capacity(geometry_column.len())),
      Box::new(Float64Builder::with_capacity(geometry_column.len())),
    ],
  );
  let mut pbf_builders = encodings
    .iter()
    .map(|_| BinaryBuilder::with_capacity(geometry_column.len(), geometry_column.len() * 16))
    .collect::<Vec<_>>();

  for_each_geometry_value(geometry_column.as_ref(), |bytes| {
    if let Some(bytes) = bytes {
      let payload = geometry_payload_from_wkb(bytes, geometry_type)?;
      code_builder.append_value(extent_xz_code(full_extent, payload.bounds, 20));
      bounds_builder
        .field_builder::<Float64Builder>(0)
        .context("xmin builder")?
        .append_value(payload.bounds.xmin);
      bounds_builder
        .field_builder::<Float64Builder>(1)
        .context("ymin builder")?
        .append_value(payload.bounds.ymin);
      bounds_builder
        .field_builder::<Float64Builder>(2)
        .context("xmax builder")?
        .append_value(payload.bounds.xmax);
      bounds_builder
        .field_builder::<Float64Builder>(3)
        .context("ymax builder")?
        .append_value(payload.bounds.ymax);
      bounds_builder.append(true);

      for (builder, encoding) in pbf_builders.iter_mut().zip(encodings) {
        builder.append_value(encode_geometry(&payload, encoding)?);
      }
    } else {
      code_builder.append_value(0);
      bounds_builder
        .field_builder::<Float64Builder>(0)
        .context("xmin builder")?
        .append_null();
      bounds_builder
        .field_builder::<Float64Builder>(1)
        .context("ymin builder")?
        .append_null();
      bounds_builder
        .field_builder::<Float64Builder>(2)
        .context("xmax builder")?
        .append_null();
      bounds_builder
        .field_builder::<Float64Builder>(3)
        .context("ymax builder")?
        .append_null();
      bounds_builder.append(false);

      for builder in &mut pbf_builders {
        builder.append_null();
      }
    }
    Ok(())
  })?;

  let mut struct_fields: Vec<Arc<Field>> = vec![
    Arc::new(Field::new(XZ_CODE_COLUMN, DataType::UInt64, false)),
    Arc::new(Field::new(
      BOUNDS_COLUMN,
      DataType::Struct(bounds_fields),
      true,
    )),
  ];
  let mut struct_columns: Vec<Arc<dyn Array>> = vec![
    Arc::new(code_builder.finish()),
    Arc::new(bounds_builder.finish()),
  ];
  for (mut builder, encoding) in pbf_builders.into_iter().zip(encodings) {
    struct_fields.push(Arc::new(Field::new(
      &encoding.column,
      DataType::Binary,
      true,
    )));
    struct_columns.push(Arc::new(builder.finish()));
  }

  columns.push(Arc::new(StructArray::try_new(
    Fields::from(struct_fields),
    struct_columns,
    None,
  )?));
  Ok(())
}

fn append_non_point_payload_columns(
  geometry_values: &[Option<&[u8]>],
  geometry_type: DisplayGeometryType,
  encodings: &[GeometryEncoding],
  pbf_builders: &mut [BinaryBuilder],
) -> Result<()> {
  if geometry_values.len() >= NON_POINT_PARALLEL_MIN_ROWS && encodings.len() > 1 {
    append_non_point_payload_columns_parallel(
      geometry_values,
      geometry_type,
      encodings,
      pbf_builders,
    )
  } else {
    append_non_point_payload_columns_sequential(
      geometry_values,
      geometry_type,
      encodings,
      pbf_builders,
    )
  }
}

fn append_non_point_payload_columns_sequential(
  geometry_values: &[Option<&[u8]>],
  geometry_type: DisplayGeometryType,
  encodings: &[GeometryEncoding],
  pbf_builders: &mut [BinaryBuilder],
) -> Result<()> {
  let mut scratch = GeometryEncodeScratch::default();
  for value in geometry_values {
    match value {
      Some(bytes) => {
        let payload = flat_geometry_payload_from_wkb(bytes, geometry_type)?;
        for (builder, encoding) in pbf_builders.iter_mut().zip(encodings) {
          let encoded = encode_flat_geometry_with_scratch(&payload, encoding, &mut scratch)?;
          builder.append_value(encoded);
        }
      }
      None => append_null_non_point_payload_row(pbf_builders),
    }
  }
  Ok(())
}

fn append_non_point_payload_columns_parallel(
  geometry_values: &[Option<&[u8]>],
  geometry_type: DisplayGeometryType,
  encodings: &[GeometryEncoding],
  pbf_builders: &mut [BinaryBuilder],
) -> Result<()> {
  for window in geometry_values.chunks(NON_POINT_ENCODE_WINDOW_ROWS) {
    let chunk_results = window
      .par_chunks(NON_POINT_ENCODE_CHUNK_ROWS)
      .map_init(NonPointEncodeScratch::default, |scratch, chunk| {
        encode_non_point_rows(chunk, geometry_type, encodings, scratch)
      })
      .collect::<Vec<_>>();
    for rows in chunk_results {
      for row in rows? {
        append_encoded_non_point_row(row, pbf_builders);
      }
    }
  }
  Ok(())
}

fn encode_non_point_rows(
  geometry_values: &[Option<&[u8]>],
  geometry_type: DisplayGeometryType,
  encodings: &[GeometryEncoding],
  scratch: &mut NonPointEncodeScratch,
) -> Result<Vec<EncodedNonPointRow>> {
  let mut rows = Vec::with_capacity(geometry_values.len());
  for value in geometry_values {
    match value {
      Some(bytes) => {
        let payload = flat_geometry_payload_from_wkb(bytes, geometry_type)?;
        let mut encoded_levels = Vec::with_capacity(encodings.len());
        for encoding in encodings {
          encoded_levels.push(encode_flat_geometry_owned_with_scratch(
            &payload,
            encoding,
            &mut scratch.encoder,
          )?);
        }
        rows.push(EncodedNonPointRow::Values(encoded_levels));
      }
      None => rows.push(EncodedNonPointRow::Null),
    }
  }
  Ok(rows)
}

fn append_encoded_non_point_row(row: EncodedNonPointRow, pbf_builders: &mut [BinaryBuilder]) {
  match row {
    EncodedNonPointRow::Values(encoded_levels) => {
      for (builder, encoded) in pbf_builders.iter_mut().zip(encoded_levels) {
        builder.append_value(encoded);
      }
    }
    EncodedNonPointRow::Null => append_null_non_point_payload_row(pbf_builders),
  }
}

fn append_null_non_point_payload_row(pbf_builders: &mut [BinaryBuilder]) {
  for builder in pbf_builders {
    builder.append_null();
  }
}

fn estimated_pbf_builder_bytes(geometry_values: &[Option<&[u8]>]) -> usize {
  if geometry_values.is_empty() {
    return 0;
  }
  let total_geometry_bytes = geometry_values
    .iter()
    .flatten()
    .map(|bytes| bytes.len())
    .sum::<usize>();
  geometry_values
    .len()
    .saturating_mul(16)
    .max(total_geometry_bytes / 8)
    .max(1024)
}

fn collect_geometry_values<'a>(array: &'a dyn Array) -> Result<Vec<Option<&'a [u8]>>> {
  match array.data_type() {
    DataType::Binary => {
      let array = array
        .as_any()
        .downcast_ref::<BinaryArray>()
        .context("geometry column is not a binary array")?;
      Ok(
        (0..array.len())
          .map(|index| (!array.is_null(index)).then(|| array.value(index)))
          .collect(),
      )
    }
    DataType::LargeBinary => {
      let array = array
        .as_any()
        .downcast_ref::<LargeBinaryArray>()
        .context("geometry column is not a large binary array")?;
      Ok(
        (0..array.len())
          .map(|index| (!array.is_null(index)).then(|| array.value(index)))
          .collect(),
      )
    }
    DataType::BinaryView => {
      let array = array
        .as_any()
        .downcast_ref::<BinaryViewArray>()
        .context("geometry column is not a binary view array")?;
      Ok(
        (0..array.len())
          .map(|index| array.is_valid(index).then(|| array.value(index)))
          .collect(),
      )
    }
    other => Err(anyhow::anyhow!(
      "unsupported geometry data type for display transform: {other}"
    )),
  }
}

#[derive(Default)]
struct NonPointEncodeScratch {
  encoder: GeometryEncodeScratch,
}

enum EncodedNonPointRow {
  Null,
  Values(Vec<Vec<u8>>),
}

fn for_each_geometry_value(
  array: &dyn Array,
  mut visitor: impl FnMut(Option<&[u8]>) -> Result<()>,
) -> Result<()> {
  match array.data_type() {
    DataType::Binary => {
      let array = array
        .as_any()
        .downcast_ref::<BinaryArray>()
        .context("geometry column is not a binary array")?;
      for index in 0..array.len() {
        let value = if array.is_null(index) {
          None
        } else {
          Some(array.value(index))
        };
        visitor(value)?;
      }
    }
    DataType::LargeBinary => {
      let array = array
        .as_any()
        .downcast_ref::<LargeBinaryArray>()
        .context("geometry column is not a large binary array")?;
      for index in 0..array.len() {
        let value = if array.is_null(index) {
          None
        } else {
          Some(array.value(index))
        };
        visitor(value)?;
      }
    }
    DataType::BinaryView => {
      let array = array
        .as_any()
        .downcast_ref::<BinaryViewArray>()
        .context("geometry column is not a binary view array")?;
      for (index, item) in array.iter().enumerate() {
        visitor(item.map(|_| array.value(index)))?;
      }
    }
    other => {
      return Err(anyhow::anyhow!(
        "unsupported geometry data type for display transform: {other}"
      ));
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use arrow_array::{BinaryArray, StringArray};
  use arrow_schema::{Field, Schema};
  use geo_types::{Geometry, Point, polygon};
  use wkb::writer::WriteOptions;

  use super::*;
  use crate::analysis::{DisplayGeometryType, DisplayJobAnalysis, Extent2D, GeometryFamily};
  use crate::geometry::{GeometryEncoding as InputGeometryEncoding, GeometryKind, GeometrySpec};
  use crate::multiscale::{DISPLAY_OUTPUT_WKID, create_geometry_encodings};

  #[test]
  fn appends_point_columns() {
    let schema = Arc::new(Schema::new(vec![
      Field::new("name", DataType::Utf8, false),
      Field::new("geometry", DataType::Binary, true),
    ]));
    let point = Geometry::Point(Point::new(1.0, 2.0));
    let mut buffer = Vec::new();
    wkb::writer::write_geometry(&mut buffer, &point, &WriteOptions::default()).unwrap();
    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(StringArray::from(vec!["a"])),
        Arc::new(BinaryArray::from(vec![Some(buffer.as_slice())])),
      ],
    )
    .unwrap();

    let analysis = DisplayJobAnalysis {
      geometry_spec: GeometrySpec {
        column: "geometry".to_string(),
        encoding: InputGeometryEncoding::Wkb,
        geometry_kind: Some(GeometryKind::Point),
      },
      geometry_family: GeometryFamily::Point,
      geometry_type: DisplayGeometryType::Point,
      spatial_reference: Default::default(),
      full_extent: Extent2D {
        xmin: 0.0,
        ymin: 0.0,
        xmax: 10.0,
        ymax: 10.0,
      },
      has_z: false,
      has_m: false,
    };

    let output_schema = build_output_schema(&schema, &analysis, &[]);
    let output = append_display_columns(&batch, &output_schema, &analysis, &[]).unwrap();
    assert!(output.column_by_name(POINT_Z_CODE_COLUMN).is_some());
    assert!(output.column_by_name(POINT_X_COLUMN).is_some());
    assert!(output.column_by_name(POINT_Y_COLUMN).is_some());
  }

  #[test]
  fn appends_non_point_struct_column() {
    let schema = Arc::new(Schema::new(vec![Field::new(
      "geometry",
      DataType::Binary,
      true,
    )]));
    let polygon = Geometry::Polygon(polygon![
        (x: 0.0, y: 0.0),
        (x: 2.0, y: 0.0),
        (x: 2.0, y: 2.0),
        (x: 0.0, y: 0.0),
    ]);
    let mut buffer = Vec::new();
    wkb::writer::write_geometry(&mut buffer, &polygon, &WriteOptions::default()).unwrap();
    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![Arc::new(BinaryArray::from(vec![Some(buffer.as_slice())]))],
    )
    .unwrap();
    let analysis = DisplayJobAnalysis {
      geometry_spec: GeometrySpec {
        column: "geometry".to_string(),
        encoding: InputGeometryEncoding::Wkb,
        geometry_kind: Some(GeometryKind::Polygon),
      },
      geometry_family: GeometryFamily::NonPoint,
      geometry_type: DisplayGeometryType::Polygon,
      spatial_reference: Default::default(),
      full_extent: Extent2D {
        xmin: 0.0,
        ymin: 0.0,
        xmax: 10.0,
        ymax: 10.0,
      },
      has_z: false,
      has_m: false,
    };
    let encodings = create_geometry_encodings(DISPLAY_OUTPUT_WKID, analysis.geometry_type).unwrap();
    let output_schema = build_output_schema(&schema, &analysis, &encodings);
    let output = append_display_columns(&batch, &output_schema, &analysis, &encodings).unwrap();
    let display = output.column_by_name(DISPLAY_COLUMN).unwrap();
    assert_eq!(display.data_type(), output_schema.field(1).data_type());
  }
}
