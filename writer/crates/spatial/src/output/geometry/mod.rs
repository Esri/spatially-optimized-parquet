//! Exposes shared Arrow mechanics for geometry output domains.

mod arrow;

pub(crate) use arrow::{
  BinaryValueAccess, geometry_signature, map_geometry_to_binary, map_geometry_to_u64,
  to_datafusion_error,
};
