use std::any::Any;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use arrow_array::builder::BinaryBuilder;
use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::{
  as_binary_array, as_binary_view_array, as_float64_array, as_large_binary_array, as_uint64_array,
};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionContext;
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, TypeSignature,
  Volatility,
};
use datafusion::prelude::{col, lit};

use crate::analysis::{DisplayGeometryType, Extent2D};
use crate::codes::{DEFAULT_COORDINATE_PRECISION, extent_xz_code, point_z_code};
use crate::display::{
  BOUNDS_COLUMN, COVERING_BBOX_COLUMN, DISPLAY_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_XZ_CODE_COLUMN, TEMP_YMAX_COLUMN,
  TEMP_YMIN_COLUMN, XZ_CODE_COLUMN,
};
use crate::multiscale::GeometryEncoding;
use crate::pbf::{
  GeometryEncodeScratch, encode_flat_geometry_with_scratch,
  flat_geometry_payload_from_wkb as pbf_flat_geometry_payload_from_wkb,
  geometry_extent_from_wkb as pbf_geometry_extent_from_wkb,
  point_xy_from_wkb as pbf_point_xy_from_wkb,
};
use crate::reprojection::{PreparedTransform, TransformSpec};

pub fn register_display_udfs(ctx: &SessionContext) {
  for udf in [
    point_zcode_udf(),
    point_x_udf(),
    point_y_udf(),
    non_point_xzcode_udf(),
    bounds_xmin_udf(),
    bounds_ymin_udf(),
    bounds_xmax_udf(),
    bounds_ymax_udf(),
  ] {
    ctx.register_udf(udf);
  }
}

pub fn point_zcode_expr(geometry_column: &str, full_extent: Extent2D) -> Expr {
  point_zcode_udf()
    .call(vec![
      col(geometry_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(POINT_Z_CODE_COLUMN)
}

pub fn point_x_expr(geometry_column: &str) -> Expr {
  point_x_udf()
    .call(vec![col(geometry_column)])
    .alias(POINT_X_COLUMN)
}

pub fn point_y_expr(geometry_column: &str) -> Expr {
  point_y_udf()
    .call(vec![col(geometry_column)])
    .alias(POINT_Y_COLUMN)
}

pub fn transformed_point_coords_expr(geometry_column: &str, transform: &TransformSpec) -> Expr {
  transformed_point_coords_udf(transform.clone()).call(vec![col(geometry_column)])
}

pub fn point_zcode_from_xy_expr(x_column: &str, y_column: &str, full_extent: Extent2D) -> Expr {
  point_zcode_from_xy_udf()
    .call(vec![
      col(x_column),
      col(y_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(POINT_Z_CODE_COLUMN)
}

pub fn non_point_xzcode_expr(geometry_column: &str, full_extent: Extent2D) -> Expr {
  non_point_xzcode_udf()
    .call(vec![
      col(geometry_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(TEMP_XZ_CODE_COLUMN)
}

pub fn non_point_geodisplay_expr(
  geometry_column: &str,
  geometry_type: DisplayGeometryType,
  encodings: &[GeometryEncoding],
) -> Expr {
  non_point_geodisplay_udf(geometry_type, encodings.to_vec())
    .call(vec![
      col(geometry_column),
      col(TEMP_XZ_CODE_COLUMN),
      col(TEMP_XMIN_COLUMN),
      col(TEMP_YMIN_COLUMN),
      col(TEMP_XMAX_COLUMN),
      col(TEMP_YMAX_COLUMN),
    ])
    .alias(DISPLAY_COLUMN)
}

pub fn bounds_xmin_expr(geometry_column: &str) -> Expr {
  bounds_xmin_udf()
    .call(vec![col(geometry_column)])
    .alias(TEMP_XMIN_COLUMN)
}

pub fn bounds_ymin_expr(geometry_column: &str) -> Expr {
  bounds_ymin_udf()
    .call(vec![col(geometry_column)])
    .alias(TEMP_YMIN_COLUMN)
}

pub fn bounds_xmax_expr(geometry_column: &str) -> Expr {
  bounds_xmax_udf()
    .call(vec![col(geometry_column)])
    .alias(TEMP_XMAX_COLUMN)
}

pub fn bounds_ymax_expr(geometry_column: &str) -> Expr {
  bounds_ymax_udf()
    .call(vec![col(geometry_column)])
    .alias(TEMP_YMAX_COLUMN)
}

pub fn transformed_bounds_expr(
  geometry_column: &str,
  geometry_type: DisplayGeometryType,
  transform: &TransformSpec,
) -> Expr {
  transformed_bounds_udf(transform.clone(), geometry_type).call(vec![col(geometry_column)])
}

pub fn non_point_xzcode_from_bounds_expr(
  xmin_column: &str,
  ymin_column: &str,
  xmax_column: &str,
  ymax_column: &str,
  full_extent: Extent2D,
) -> Expr {
  non_point_xzcode_from_bounds_udf()
    .call(vec![
      col(xmin_column),
      col(ymin_column),
      col(xmax_column),
      col(ymax_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(TEMP_XZ_CODE_COLUMN)
}

pub fn feature_bbox_expr(
  geometry_column: &str,
  xmin_column: &str,
  ymin_column: &str,
  xmax_column: &str,
  ymax_column: &str,
) -> Expr {
  feature_bbox_udf()
    .call(vec![
      col(geometry_column),
      col(xmin_column),
      col(ymin_column),
      col(xmax_column),
      col(ymax_column),
    ])
    .alias(COVERING_BBOX_COLUMN)
}

pub fn reproject_geometry_expr(geometry_column: &str, transform: &TransformSpec) -> Expr {
  reproject_geometry_udf(transform.clone()).call(vec![col(geometry_column)])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FloatUdfKind {
  PointX,
  PointY,
  BoundsXmin,
  BoundsYmin,
  BoundsXmax,
  BoundsYmax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GeometryFloatUdf {
  name: &'static str,
  kind: FloatUdfKind,
}

impl ScalarUDFImpl for GeometryFloatUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    self.name
  }

  fn signature(&self) -> &Signature {
    unary_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Float64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let output = match self.kind {
      FloatUdfKind::PointX => map_geometry_to_f64(geometry, |bytes| {
        Ok(Some(
          bytes
            .map(point_xy_from_wkb)
            .transpose()?
            .map(|(x, _)| x)
            .unwrap_or(f64::NAN),
        ))
      })?,
      FloatUdfKind::PointY => map_geometry_to_f64(geometry, |bytes| {
        Ok(Some(
          bytes
            .map(point_xy_from_wkb)
            .transpose()?
            .map(|(_, y)| y)
            .unwrap_or(f64::NAN),
        ))
      })?,
      FloatUdfKind::BoundsXmin => map_geometry_to_f64(geometry, |bytes| {
        Ok(
          bytes
            .map(extent_from_wkb)
            .transpose()?
            .map(|extent| extent.xmin),
        )
      })?,
      FloatUdfKind::BoundsYmin => map_geometry_to_f64(geometry, |bytes| {
        Ok(
          bytes
            .map(extent_from_wkb)
            .transpose()?
            .map(|extent| extent.ymin),
        )
      })?,
      FloatUdfKind::BoundsXmax => map_geometry_to_f64(geometry, |bytes| {
        Ok(
          bytes
            .map(extent_from_wkb)
            .transpose()?
            .map(|extent| extent.xmax),
        )
      })?,
      FloatUdfKind::BoundsYmax => map_geometry_to_f64(geometry, |bytes| {
        Ok(
          bytes
            .map(extent_from_wkb)
            .transpose()?
            .map(|extent| extent.ymax),
        )
      })?,
    };
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CodeUdfKind {
  PointZCode,
  NonPointXzCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CodeFromColumnsUdfKind {
  PointZCode,
  NonPointXzCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CodeFromColumnsUdf {
  name: &'static str,
  kind: CodeFromColumnsUdfKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GeometryCodeUdf {
  name: &'static str,
  kind: CodeUdfKind,
}

impl ScalarUDFImpl for GeometryCodeUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    self.name
  }

  fn signature(&self) -> &Signature {
    code_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let full_extent = extent_from_arg_arrays(&arrays)?;
    let output = map_geometry_to_u64(geometry, |bytes| match (self.kind, bytes) {
      (CodeUdfKind::PointZCode, Some(bytes)) => {
        let (x, y) = point_xy_from_wkb(bytes)?;
        Ok(point_z_code(
          full_extent,
          x,
          y,
          DEFAULT_COORDINATE_PRECISION,
        ))
      }
      (CodeUdfKind::NonPointXzCode, Some(bytes)) => {
        Ok(extent_xz_code(full_extent, extent_from_wkb(bytes)?, 20))
      }
      (_, None) => Ok(0),
    })?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TransformedPointCoordsUdf {
  transform: TransformSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TransformedBoundsUdf {
  transform: TransformSpec,
  geometry_type: DisplayGeometryType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReprojectGeometryUdf {
  transform: TransformSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FeatureBboxUdf;

impl ScalarUDFImpl for TransformedPointCoordsUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "display_transformed_point_coords"
  }

  fn signature(&self) -> &Signature {
    unary_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(point_coords_fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let prepared = self.transform.prepare().map_err(to_datafusion_error)?;
    let output = match geometry.data_type() {
      DataType::Binary => {
        transformed_point_coords_struct(as_binary_array(geometry.as_ref())?, &prepared)?
      }
      DataType::LargeBinary => {
        transformed_point_coords_struct(as_large_binary_array(geometry.as_ref())?, &prepared)?
      }
      DataType::BinaryView => {
        transformed_point_coords_struct(as_binary_view_array(geometry.as_ref())?, &prepared)?
      }
      other => {
        return Err(DataFusionError::Execution(format!(
          "unsupported geometry data type for UDF: {other}"
        )));
      }
    };
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

impl ScalarUDFImpl for TransformedBoundsUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "display_transformed_bounds"
  }

  fn signature(&self) -> &Signature {
    unary_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(bounds_struct_fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let prepared = self.transform.prepare().map_err(to_datafusion_error)?;
    let output = match geometry.data_type() {
      DataType::Binary => transformed_bounds_struct(
        as_binary_array(geometry.as_ref())?,
        &prepared,
        self.geometry_type,
      )?,
      DataType::LargeBinary => transformed_bounds_struct(
        as_large_binary_array(geometry.as_ref())?,
        &prepared,
        self.geometry_type,
      )?,
      DataType::BinaryView => transformed_bounds_struct(
        as_binary_view_array(geometry.as_ref())?,
        &prepared,
        self.geometry_type,
      )?,
      other => {
        return Err(DataFusionError::Execution(format!(
          "unsupported geometry data type for UDF: {other}"
        )));
      }
    };
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

impl ScalarUDFImpl for ReprojectGeometryUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "display_reproject_geometry"
  }

  fn signature(&self) -> &Signature {
    unary_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Binary)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let prepared = self.transform.prepare().map_err(to_datafusion_error)?;
    let output = map_geometry_to_binary(geometry, |bytes| match bytes {
      Some(bytes) => prepared
        .reproject_wkb(bytes)
        .map(Some)
        .map_err(to_datafusion_error),
      None => Ok(None),
    })?;
    Ok(ColumnarValue::Array(output))
  }
}

impl ScalarUDFImpl for FeatureBboxUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "display_feature_bbox"
  }

  fn signature(&self) -> &Signature {
    feature_bbox_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(bounds_struct_fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let xmin = as_float64_array(
      arrays
        .get(1)
        .ok_or_else(|| DataFusionError::Execution("missing xmin argument".to_string()))?
        .as_ref(),
    )?;
    let ymin = as_float64_array(
      arrays
        .get(2)
        .ok_or_else(|| DataFusionError::Execution("missing ymin argument".to_string()))?
        .as_ref(),
    )?;
    let xmax = as_float64_array(
      arrays
        .get(3)
        .ok_or_else(|| DataFusionError::Execution("missing xmax argument".to_string()))?
        .as_ref(),
    )?;
    let ymax = as_float64_array(
      arrays
        .get(4)
        .ok_or_else(|| DataFusionError::Execution("missing ymax argument".to_string()))?
        .as_ref(),
    )?;
    let output = feature_bbox_struct(geometry, xmin, ymin, xmax, ymax)?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

impl ScalarUDFImpl for CodeFromColumnsUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    self.name
  }

  fn signature(&self) -> &Signature {
    match self.kind {
      CodeFromColumnsUdfKind::PointZCode => point_zcode_from_xy_signature(),
      CodeFromColumnsUdfKind::NonPointXzCode => non_point_xzcode_from_bounds_signature(),
    }
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let output = match self.kind {
      CodeFromColumnsUdfKind::PointZCode => {
        let x = as_float64_array(arrays[0].as_ref())?;
        let y = as_float64_array(arrays[1].as_ref())?;
        let full_extent = Extent2D {
          xmin: array_first_f64(&arrays[2])?,
          ymin: array_first_f64(&arrays[3])?,
          xmax: array_first_f64(&arrays[4])?,
          ymax: array_first_f64(&arrays[5])?,
        };
        let mut values = Vec::with_capacity(x.len());
        for index in 0..x.len() {
          values.push(if x.is_null(index) || y.is_null(index) {
            0
          } else {
            point_z_code(
              full_extent,
              x.value(index),
              y.value(index),
              DEFAULT_COORDINATE_PRECISION,
            )
          });
        }
        UInt64Array::from(values)
      }
      CodeFromColumnsUdfKind::NonPointXzCode => {
        let xmin = as_float64_array(arrays[0].as_ref())?;
        let ymin = as_float64_array(arrays[1].as_ref())?;
        let xmax = as_float64_array(arrays[2].as_ref())?;
        let ymax = as_float64_array(arrays[3].as_ref())?;
        let full_extent = Extent2D {
          xmin: array_first_f64(&arrays[4])?,
          ymin: array_first_f64(&arrays[5])?,
          xmax: array_first_f64(&arrays[6])?,
          ymax: array_first_f64(&arrays[7])?,
        };
        let mut values = Vec::with_capacity(xmin.len());
        for index in 0..xmin.len() {
          values.push(
            if xmin.is_null(index)
              || ymin.is_null(index)
              || xmax.is_null(index)
              || ymax.is_null(index)
            {
              0
            } else {
              extent_xz_code(
                full_extent,
                Extent2D {
                  xmin: xmin.value(index),
                  ymin: ymin.value(index),
                  xmax: xmax.value(index),
                  ymax: ymax.value(index),
                },
                20,
              )
            },
          );
        }
        UInt64Array::from(values)
      }
    };
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone)]
struct NonPointGeodisplayUdf {
  geometry_type: DisplayGeometryType,
  encodings: Vec<GeometryEncoding>,
  display_fields: Fields,
  bounds_fields: Fields,
}

impl NonPointGeodisplayUdf {
  fn new(geometry_type: DisplayGeometryType, encodings: Vec<GeometryEncoding>) -> Self {
    let bounds_fields = Fields::from(vec![
      Arc::new(Field::new("xmin", DataType::Float64, true)),
      Arc::new(Field::new("ymin", DataType::Float64, true)),
      Arc::new(Field::new("xmax", DataType::Float64, true)),
      Arc::new(Field::new("ymax", DataType::Float64, true)),
    ]);
    let mut display_fields = vec![
      Arc::new(Field::new(XZ_CODE_COLUMN, DataType::UInt64, false)),
      Arc::new(Field::new(
        BOUNDS_COLUMN,
        DataType::Struct(bounds_fields.clone()),
        true,
      )),
    ];
    for encoding in &encodings {
      display_fields.push(Arc::new(Field::new(
        &encoding.column,
        DataType::Binary,
        true,
      )));
    }
    Self {
      geometry_type,
      encodings,
      display_fields: Fields::from(display_fields),
      bounds_fields,
    }
  }

  fn build_display<T: BinaryValueAccess>(
    &self,
    geometry: &T,
    xz_code: &UInt64Array,
    xmin: &Float64Array,
    ymin: &Float64Array,
    xmax: &Float64Array,
    ymax: &Float64Array,
  ) -> DataFusionResult<StructArray> {
    let mut pbf_builders = self
      .encodings
      .iter()
      .map(|_| BinaryBuilder::with_capacity(geometry.len(), geometry.len() * 16))
      .collect::<Vec<_>>();
    let mut scratch = GeometryEncodeScratch::default();

    for index in 0..geometry.len() {
      match geometry.value_opt(index) {
        Some(bytes) => {
          let payload = pbf_flat_geometry_payload_from_wkb(bytes, self.geometry_type)
            .map_err(to_datafusion_error)?;
          for (builder, encoding) in pbf_builders.iter_mut().zip(&self.encodings) {
            let encoded = encode_flat_geometry_with_scratch(&payload, encoding, &mut scratch)
              .map_err(to_datafusion_error)?;
            builder.append_value(encoded);
          }
        }
        None => {
          for builder in &mut pbf_builders {
            builder.append_null();
          }
        }
      }
    }

    let bounds = StructArray::try_new(
      self.bounds_fields.clone(),
      vec![
        Arc::new(xmin.clone()),
        Arc::new(ymin.clone()),
        Arc::new(xmax.clone()),
        Arc::new(ymax.clone()),
      ],
      xmin.nulls().cloned(),
    )
    .map_err(to_datafusion_error)?;

    let mut display_columns: Vec<ArrayRef> = vec![Arc::new(xz_code.clone()), Arc::new(bounds)];
    for mut builder in pbf_builders {
      display_columns.push(Arc::new(builder.finish()));
    }

    StructArray::try_new(self.display_fields.clone(), display_columns, None)
      .map_err(to_datafusion_error)
  }
}

impl PartialEq for NonPointGeodisplayUdf {
  fn eq(&self, other: &Self) -> bool {
    self.geometry_type == other.geometry_type
      && self.encodings.len() == other.encodings.len()
      && self
        .encodings
        .iter()
        .zip(&other.encodings)
        .all(|(left, right)| {
          left.level == right.level
            && left.column == right.column
            && left.resolution.to_bits() == right.resolution.to_bits()
            && left.scale.to_bits() == right.scale.to_bits()
            && left
              .transform
              .scale
              .iter()
              .map(|value| value.to_bits())
              .eq(right.transform.scale.iter().map(|value| value.to_bits()))
            && left
              .transform
              .translate
              .iter()
              .map(|value| value.to_bits())
              .eq(
                right
                  .transform
                  .translate
                  .iter()
                  .map(|value| value.to_bits()),
              )
            && left.min_length == right.min_length
        })
  }
}

impl Eq for NonPointGeodisplayUdf {}

impl Hash for NonPointGeodisplayUdf {
  fn hash<H: Hasher>(&self, state: &mut H) {
    match self.geometry_type {
      DisplayGeometryType::Point => 0u8,
      DisplayGeometryType::MultiPoint => 1u8,
      DisplayGeometryType::Polyline => 2u8,
      DisplayGeometryType::Polygon => 3u8,
    }
    .hash(state);
    for encoding in &self.encodings {
      encoding.level.hash(state);
      encoding.column.hash(state);
      encoding.resolution.to_bits().hash(state);
      encoding.scale.to_bits().hash(state);
      for value in encoding.transform.scale {
        value.to_bits().hash(state);
      }
      for value in encoding.transform.translate {
        value.to_bits().hash(state);
      }
      encoding.min_length.hash(state);
    }
  }
}

impl ScalarUDFImpl for NonPointGeodisplayUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "display_nonpoint_geodisplay"
  }

  fn signature(&self) -> &Signature {
    non_point_geodisplay_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(self.display_fields.clone()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let xz_code = as_uint64_array(arrays[1].as_ref())?;
    let xmin = as_float64_array(arrays[2].as_ref())?;
    let ymin = as_float64_array(arrays[3].as_ref())?;
    let xmax = as_float64_array(arrays[4].as_ref())?;
    let ymax = as_float64_array(arrays[5].as_ref())?;

    let output = match geometry.data_type() {
      DataType::Binary => self.build_display(
        as_binary_array(geometry.as_ref())?,
        xz_code,
        xmin,
        ymin,
        xmax,
        ymax,
      )?,
      DataType::LargeBinary => self.build_display(
        as_large_binary_array(geometry.as_ref())?,
        xz_code,
        xmin,
        ymin,
        xmax,
        ymax,
      )?,
      DataType::BinaryView => self.build_display(
        as_binary_view_array(geometry.as_ref())?,
        xz_code,
        xmin,
        ymin,
        xmax,
        ymax,
      )?,
      other => {
        return Err(DataFusionError::Execution(format!(
          "unsupported geometry data type for UDF: {other}"
        )));
      }
    };

    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

fn point_zcode_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryCodeUdf {
    name: "display_point_zcode",
    kind: CodeUdfKind::PointZCode,
  })
}

fn non_point_xzcode_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryCodeUdf {
    name: "display_nonpoint_xzcode",
    kind: CodeUdfKind::NonPointXzCode,
  })
}

fn point_x_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_point_x",
    kind: FloatUdfKind::PointX,
  })
}

fn point_y_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_point_y",
    kind: FloatUdfKind::PointY,
  })
}

fn bounds_xmin_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_bounds_xmin",
    kind: FloatUdfKind::BoundsXmin,
  })
}

fn bounds_ymin_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_bounds_ymin",
    kind: FloatUdfKind::BoundsYmin,
  })
}

fn bounds_xmax_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_bounds_xmax",
    kind: FloatUdfKind::BoundsXmax,
  })
}

fn bounds_ymax_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_bounds_ymax",
    kind: FloatUdfKind::BoundsYmax,
  })
}

fn transformed_point_coords_udf(transform: TransformSpec) -> ScalarUDF {
  ScalarUDF::new_from_impl(TransformedPointCoordsUdf { transform })
}

fn transformed_bounds_udf(
  transform: TransformSpec,
  geometry_type: DisplayGeometryType,
) -> ScalarUDF {
  ScalarUDF::new_from_impl(TransformedBoundsUdf {
    transform,
    geometry_type,
  })
}

fn reproject_geometry_udf(transform: TransformSpec) -> ScalarUDF {
  ScalarUDF::new_from_impl(ReprojectGeometryUdf { transform })
}

fn feature_bbox_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(FeatureBboxUdf)
}

fn point_zcode_from_xy_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(CodeFromColumnsUdf {
    name: "display_point_zcode_from_xy",
    kind: CodeFromColumnsUdfKind::PointZCode,
  })
}

fn non_point_xzcode_from_bounds_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(CodeFromColumnsUdf {
    name: "display_nonpoint_xzcode_from_bounds",
    kind: CodeFromColumnsUdfKind::NonPointXzCode,
  })
}

fn non_point_geodisplay_udf(
  geometry_type: DisplayGeometryType,
  encodings: Vec<GeometryEncoding>,
) -> ScalarUDF {
  ScalarUDF::new_from_impl(NonPointGeodisplayUdf::new(geometry_type, encodings))
}

fn unary_geometry_signature() -> &'static Signature {
  static SIGNATURE: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
  SIGNATURE.get_or_init(|| {
    Signature::one_of(
      vec![
        TypeSignature::Exact(vec![DataType::Binary]),
        TypeSignature::Exact(vec![DataType::LargeBinary]),
        TypeSignature::Exact(vec![DataType::BinaryView]),
      ],
      Volatility::Immutable,
    )
  })
}

fn non_point_geodisplay_signature() -> &'static Signature {
  static SIGNATURE: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
  SIGNATURE.get_or_init(|| {
    Signature::one_of(
      vec![
        TypeSignature::Exact(vec![
          DataType::Binary,
          DataType::UInt64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
        TypeSignature::Exact(vec![
          DataType::LargeBinary,
          DataType::UInt64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
        TypeSignature::Exact(vec![
          DataType::BinaryView,
          DataType::UInt64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
      ],
      Volatility::Immutable,
    )
  })
}

fn point_zcode_from_xy_signature() -> &'static Signature {
  static SIGNATURE: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
  SIGNATURE.get_or_init(|| {
    Signature::exact(
      vec![
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
      ],
      Volatility::Immutable,
    )
  })
}

fn non_point_xzcode_from_bounds_signature() -> &'static Signature {
  static SIGNATURE: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
  SIGNATURE.get_or_init(|| {
    Signature::exact(
      vec![
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
        DataType::Float64,
      ],
      Volatility::Immutable,
    )
  })
}

fn feature_bbox_signature() -> &'static Signature {
  static SIGNATURE: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
  SIGNATURE.get_or_init(|| {
    Signature::one_of(
      vec![
        TypeSignature::Exact(vec![
          DataType::Binary,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
        TypeSignature::Exact(vec![
          DataType::LargeBinary,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
        TypeSignature::Exact(vec![
          DataType::BinaryView,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
      ],
      Volatility::Immutable,
    )
  })
}

fn code_geometry_signature() -> &'static Signature {
  static SIGNATURE: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
  SIGNATURE.get_or_init(|| {
    Signature::one_of(
      vec![
        TypeSignature::Exact(vec![
          DataType::Binary,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
        TypeSignature::Exact(vec![
          DataType::LargeBinary,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
        TypeSignature::Exact(vec![
          DataType::BinaryView,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ]),
      ],
      Volatility::Immutable,
    )
  })
}

fn point_coords_fields() -> Fields {
  static FIELDS: std::sync::OnceLock<Fields> = std::sync::OnceLock::new();
  FIELDS
    .get_or_init(|| {
      Fields::from(vec![
        Arc::new(Field::new("x", DataType::Float64, true)),
        Arc::new(Field::new("y", DataType::Float64, true)),
      ])
    })
    .clone()
}

fn bounds_struct_fields() -> Fields {
  static FIELDS: std::sync::OnceLock<Fields> = std::sync::OnceLock::new();
  FIELDS
    .get_or_init(|| {
      Fields::from(vec![
        Arc::new(Field::new("xmin", DataType::Float64, true)),
        Arc::new(Field::new("ymin", DataType::Float64, true)),
        Arc::new(Field::new("xmax", DataType::Float64, true)),
        Arc::new(Field::new("ymax", DataType::Float64, true)),
      ])
    })
    .clone()
}

fn map_geometry_to_f64(
  geometry: &ArrayRef,
  evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<Option<f64>>,
) -> DataFusionResult<Float64Array> {
  match geometry.data_type() {
    DataType::Binary => map_binary_like_to_f64(as_binary_array(geometry.as_ref())?, evaluator),
    DataType::LargeBinary => {
      map_binary_like_to_f64(as_large_binary_array(geometry.as_ref())?, evaluator)
    }
    DataType::BinaryView => {
      map_binary_like_to_f64(as_binary_view_array(geometry.as_ref())?, evaluator)
    }
    other => Err(DataFusionError::Execution(format!(
      "unsupported geometry data type for UDF: {other}"
    ))),
  }
}

fn map_binary_like_to_f64<T>(
  array: &T,
  mut evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<Option<f64>>,
) -> DataFusionResult<Float64Array>
where
  T: BinaryValueAccess,
{
  let mut values = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    values.push(evaluator(array.value_opt(index))?);
  }
  Ok(Float64Array::from(values))
}

fn map_geometry_to_u64(
  geometry: &ArrayRef,
  evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<u64>,
) -> DataFusionResult<UInt64Array> {
  match geometry.data_type() {
    DataType::Binary => map_binary_like_to_u64(as_binary_array(geometry.as_ref())?, evaluator),
    DataType::LargeBinary => {
      map_binary_like_to_u64(as_large_binary_array(geometry.as_ref())?, evaluator)
    }
    DataType::BinaryView => {
      map_binary_like_to_u64(as_binary_view_array(geometry.as_ref())?, evaluator)
    }
    other => Err(DataFusionError::Execution(format!(
      "unsupported geometry data type for UDF: {other}"
    ))),
  }
}

fn map_binary_like_to_u64<T>(
  array: &T,
  mut evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<u64>,
) -> DataFusionResult<UInt64Array>
where
  T: BinaryValueAccess,
{
  let mut values = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    values.push(evaluator(array.value_opt(index))?);
  }
  Ok(UInt64Array::from(values))
}

fn map_geometry_to_binary(
  geometry: &ArrayRef,
  evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<Option<Vec<u8>>>,
) -> DataFusionResult<ArrayRef> {
  match geometry.data_type() {
    DataType::Binary => map_binary_like_to_binary(as_binary_array(geometry.as_ref())?, evaluator),
    DataType::LargeBinary => {
      map_binary_like_to_binary(as_large_binary_array(geometry.as_ref())?, evaluator)
    }
    DataType::BinaryView => {
      map_binary_like_to_binary(as_binary_view_array(geometry.as_ref())?, evaluator)
    }
    other => Err(DataFusionError::Execution(format!(
      "unsupported geometry data type for UDF: {other}"
    ))),
  }
}

fn map_binary_like_to_binary<T>(
  array: &T,
  mut evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<Option<Vec<u8>>>,
) -> DataFusionResult<ArrayRef>
where
  T: BinaryValueAccess,
{
  let mut builder = BinaryBuilder::with_capacity(array.len(), array.len() * 16);
  for index in 0..array.len() {
    match evaluator(array.value_opt(index))? {
      Some(bytes) => builder.append_value(bytes),
      None => builder.append_null(),
    }
  }
  Ok(Arc::new(builder.finish()))
}

fn feature_bbox_struct(
  geometry: &ArrayRef,
  xmin: &Float64Array,
  ymin: &Float64Array,
  xmax: &Float64Array,
  ymax: &Float64Array,
) -> DataFusionResult<StructArray> {
  StructArray::try_new(
    bounds_struct_fields(),
    vec![
      Arc::new(xmin.clone()),
      Arc::new(ymin.clone()),
      Arc::new(xmax.clone()),
      Arc::new(ymax.clone()),
    ],
    geometry.nulls().cloned(),
  )
  .map_err(to_datafusion_error)
}

fn transformed_point_coords_struct<T>(
  array: &T,
  transform: &PreparedTransform,
) -> DataFusionResult<StructArray>
where
  T: BinaryValueAccess,
{
  let mut xs = Vec::with_capacity(array.len());
  let mut ys = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    match array.value_opt(index) {
      Some(bytes) => {
        let (x, y) = point_xy_from_wkb(bytes)?;
        let (x, y) = transform
          .transform_point(x, y)
          .map_err(to_datafusion_error)?;
        xs.push(Some(x));
        ys.push(Some(y));
      }
      None => {
        xs.push(None);
        ys.push(None);
      }
    }
  }
  let x_array = Float64Array::from(xs);
  let y_array = Float64Array::from(ys);
  StructArray::try_new(
    point_coords_fields(),
    vec![Arc::new(x_array.clone()), Arc::new(y_array)],
    x_array.nulls().cloned(),
  )
  .map_err(to_datafusion_error)
}

fn transformed_bounds_struct<T>(
  array: &T,
  transform: &PreparedTransform,
  geometry_type: DisplayGeometryType,
) -> DataFusionResult<StructArray>
where
  T: BinaryValueAccess,
{
  let mut xmin_values = Vec::with_capacity(array.len());
  let mut ymin_values = Vec::with_capacity(array.len());
  let mut xmax_values = Vec::with_capacity(array.len());
  let mut ymax_values = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    match array.value_opt(index) {
      Some(bytes) => {
        let extent = transform
          .transform_geometry_bounds_from_wkb(bytes, geometry_type)
          .map_err(to_datafusion_error)?;
        xmin_values.push(Some(extent.xmin));
        ymin_values.push(Some(extent.ymin));
        xmax_values.push(Some(extent.xmax));
        ymax_values.push(Some(extent.ymax));
      }
      None => {
        xmin_values.push(None);
        ymin_values.push(None);
        xmax_values.push(None);
        ymax_values.push(None);
      }
    }
  }
  let xmin_array = Float64Array::from(xmin_values);
  let ymin_array = Float64Array::from(ymin_values);
  let xmax_array = Float64Array::from(xmax_values);
  let ymax_array = Float64Array::from(ymax_values);
  StructArray::try_new(
    bounds_struct_fields(),
    vec![
      Arc::new(xmin_array.clone()),
      Arc::new(ymin_array),
      Arc::new(xmax_array),
      Arc::new(ymax_array),
    ],
    xmin_array.nulls().cloned(),
  )
  .map_err(to_datafusion_error)
}

trait BinaryValueAccess {
  fn len(&self) -> usize;
  fn value_opt(&self, index: usize) -> Option<&[u8]>;
}

impl BinaryValueAccess for arrow_array::BinaryArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    (!self.is_null(index)).then(|| self.value(index))
  }
}

impl BinaryValueAccess for arrow_array::LargeBinaryArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    (!self.is_null(index)).then(|| self.value(index))
  }
}

impl BinaryValueAccess for arrow_array::BinaryViewArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    self.is_valid(index).then(|| self.value(index))
  }
}

fn point_xy_from_wkb(bytes: &[u8]) -> DataFusionResult<(f64, f64)> {
  pbf_point_xy_from_wkb(bytes).map_err(to_datafusion_error)
}

fn extent_from_wkb(bytes: &[u8]) -> DataFusionResult<Extent2D> {
  pbf_geometry_extent_from_wkb(bytes).map_err(to_datafusion_error)
}

fn extent_from_arg_arrays(arrays: &[ArrayRef]) -> DataFusionResult<Extent2D> {
  let xmin = array_first_f64(&arrays[1])?;
  let ymin = array_first_f64(&arrays[2])?;
  let xmax = array_first_f64(&arrays[3])?;
  let ymax = array_first_f64(&arrays[4])?;
  Ok(Extent2D {
    xmin,
    ymin,
    xmax,
    ymax,
  })
}

fn array_first_f64(array: &ArrayRef) -> DataFusionResult<f64> {
  let array = as_float64_array(array.as_ref())?;
  if array.is_empty() || array.is_null(0) {
    return Err(DataFusionError::Execution(
      "missing full extent argument for display helper UDF".to_string(),
    ));
  }
  Ok(array.value(0))
}

fn to_datafusion_error(err: impl Into<anyhow::Error>) -> DataFusionError {
  DataFusionError::External(err.into().into())
}
