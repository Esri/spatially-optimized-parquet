//! Caches DataFusion signatures and nested Arrow fields.

use std::sync::{Arc, OnceLock};

use arrow_schema::{DataType, Field, Fields};
use datafusion::logical_expr::{Signature, TypeSignature, Volatility};
pub(super) fn unary_geometry_signature() -> &'static Signature {
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

pub(super) fn non_point_geodisplay_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
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

pub(super) fn point_zcode_from_xy_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
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

pub(super) fn non_point_xzcode_from_bounds_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
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

pub(super) fn feature_bbox_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
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

pub(super) fn code_geometry_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
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

pub(super) fn point_coords_fields() -> Fields {
  static FIELDS: OnceLock<Fields> = OnceLock::new();
  FIELDS
    .get_or_init(|| {
      Fields::from(vec![
        Arc::new(Field::new("x", DataType::Float64, true)),
        Arc::new(Field::new("y", DataType::Float64, true)),
      ])
    })
    .clone()
}

pub(super) fn bounds_struct_fields() -> Fields {
  static FIELDS: OnceLock<Fields> = OnceLock::new();
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
