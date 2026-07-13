//! Adapts Arrow binary geometry arrays for shared spatial operations.

use std::sync::{Arc, OnceLock};

use arrow_array::builder::BinaryBuilder;
use arrow_array::{Array, ArrayRef, UInt64Array};
use arrow_schema::DataType;
use datafusion::common::cast::{as_binary_array, as_binary_view_array, as_large_binary_array};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{Signature, TypeSignature, Volatility};

pub(crate) fn geometry_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
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

pub(crate) fn map_geometry_to_u64(
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

pub(crate) fn map_geometry_to_binary(
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

pub(crate) trait BinaryValueAccess {
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

pub(crate) fn to_datafusion_error(error: impl Into<anyhow::Error>) -> DataFusionError {
  DataFusionError::External(error.into().into())
}

#[cfg(test)]
mod tests {
  use super::BinaryValueAccess;

  fn assert_null_and_value(array: &impl BinaryValueAccess) {
    assert_eq!(array.len(), 2);
    assert_eq!(array.value_opt(0), Some(&b"wkb"[..]));
    assert_eq!(array.value_opt(1), None);
  }

  #[test]
  fn binary_value_access_preserves_nulls_across_arrow_encodings() {
    assert_null_and_value(&arrow_array::BinaryArray::from(vec![
      Some(&b"wkb"[..]),
      None,
    ]));
    assert_null_and_value(&arrow_array::LargeBinaryArray::from(vec![
      Some(&b"wkb"[..]),
      None,
    ]));
    assert_null_and_value(&arrow_array::BinaryViewArray::from(vec![
      Some(&b"wkb"[..]),
      None,
    ]));
  }
}
