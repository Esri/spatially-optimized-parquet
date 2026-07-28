//! Builds one multiscale level array through its physical encoding.

use arrow_array::ArrayRef;
use arrow_schema::DataType;

use super::MultiscaleEncoding;
use crate::geometry::{GeometryError, GeometryType};

use super::{GEOLOD_COLUMN, MultiscaleLevel};
use crate::geometry::{NativeGeometryArrayBuilder, PbfArrayBuilder, QuantizedGeometry};

/// Builds one multiscale level array through its selected physical encoding.
pub(crate) enum MultiscaleLevelArrayBuilder {
  Pbf(PbfArrayBuilder),
  QuantizedNative(NativeGeometryArrayBuilder),
}

impl MultiscaleEncoding {
  pub(crate) fn geometry_data_type(
    self,
    geometry_type: GeometryType,
    has_z: bool,
    has_m: bool,
  ) -> DataType {
    match self {
      Self::Pbf => DataType::Binary,
      Self::QuantizedNative => NativeGeometryArrayBuilder::data_type(geometry_type, has_z, has_m),
    }
  }

  pub(crate) fn missing_component_value(self) -> &'static str {
    match self {
      Self::Pbf => "0",
      Self::QuantizedNative => "null",
    }
  }

  pub(crate) fn delta_binary_packed_column_paths(
    self,
    levels: &[MultiscaleLevel],
    geometry_type: GeometryType,
    has_z: bool,
    has_m: bool,
  ) -> Vec<String> {
    match self {
      Self::Pbf => Vec::new(),
      Self::QuantizedNative => NativeGeometryArrayBuilder::coordinate_column_paths(
        GEOLOD_COLUMN,
        &levels
          .iter()
          .map(|level| level.column.clone())
          .collect::<Vec<_>>(),
        geometry_type,
        has_z,
        has_m,
      ),
    }
  }

  pub(crate) fn resolve_writer(
    self,
    geometry_type: GeometryType,
    has_z: bool,
    has_m: bool,
    capacity: usize,
  ) -> MultiscaleLevelArrayBuilder {
    match self {
      MultiscaleEncoding::Pbf => MultiscaleLevelArrayBuilder::Pbf(PbfArrayBuilder::new(capacity)),
      MultiscaleEncoding::QuantizedNative => MultiscaleLevelArrayBuilder::QuantizedNative(
        NativeGeometryArrayBuilder::new(geometry_type, has_z, has_m, capacity),
      ),
    }
  }
}

impl MultiscaleLevelArrayBuilder {
  pub(crate) fn append(&mut self, geometry: &QuantizedGeometry) -> Result<(), GeometryError> {
    match self {
      Self::Pbf(writer) => Ok(writer.append(geometry)?),
      Self::QuantizedNative(writer) => {
        writer.append_quantized_geometry(geometry);
        Ok(())
      }
    }
  }

  pub(crate) fn append_null(&mut self) {
    match self {
      Self::Pbf(writer) => writer.append_null(),
      Self::QuantizedNative(builder) => builder.append_null(),
    }
  }

  pub(crate) fn finish(self) -> ArrayRef {
    match self {
      Self::Pbf(writer) => writer.finish(),
      Self::QuantizedNative(mut builder) => builder.finish(),
    }
  }
}
