//! Adapts Arrow binary geometry arrays into one spatial input boundary.

use std::error::Error;
use std::sync::OnceLock;

use arrow_array::{Array, BinaryArray, BinaryViewArray, LargeBinaryArray};
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

pub(crate) enum GeometryArray<'a> {
  Binary(&'a BinaryArray),
  LargeBinary(&'a LargeBinaryArray),
  BinaryView(&'a BinaryViewArray),
}

impl<'a> GeometryArray<'a> {
  pub(crate) fn try_new(array: &'a dyn Array) -> DataFusionResult<Self> {
    match array.data_type() {
      DataType::Binary => Ok(Self::Binary(as_binary_array(array)?)),
      DataType::LargeBinary => Ok(Self::LargeBinary(as_large_binary_array(array)?)),
      DataType::BinaryView => Ok(Self::BinaryView(as_binary_view_array(array)?)),
      other => Err(DataFusionError::Execution(format!(
        "unsupported geometry data type: {other}"
      ))),
    }
  }

  pub(crate) fn len(&self) -> usize {
    match self {
      Self::Binary(array) => array.len(),
      Self::LargeBinary(array) => array.len(),
      Self::BinaryView(array) => array.len(),
    }
  }

  pub(crate) fn value(&self, index: usize) -> Option<&[u8]> {
    match self {
      Self::Binary(array) => (!array.is_null(index)).then(|| array.value(index)),
      Self::LargeBinary(array) => (!array.is_null(index)).then(|| array.value(index)),
      Self::BinaryView(array) => array.is_valid(index).then(|| array.value(index)),
    }
  }

  pub(crate) fn values(&self) -> impl Iterator<Item = Option<&[u8]>> {
    (0..self.len()).map(|index| self.value(index))
  }
}

pub(crate) fn to_datafusion_error(error: impl Error + Send + Sync + 'static) -> DataFusionError {
  DataFusionError::External(Box::new(error))
}

#[cfg(test)]
mod tests {
  use arrow_array::Array;

  use super::GeometryArray;

  fn assert_null_and_value(array: &dyn Array) {
    let geometry = GeometryArray::try_new(array).unwrap();
    assert_eq!(geometry.len(), 2);
    assert_eq!(geometry.value(0), Some(&b"wkb"[..]));
    assert_eq!(geometry.value(1), None);
  }

  #[test]
  fn geometry_array_preserves_nulls_across_arrow_encodings() {
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
