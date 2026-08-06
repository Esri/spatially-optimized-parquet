//! Builds one multiscale level array through its physical encoding.

use arrow_array::ArrayRef;
use arrow_array::types::{Float64Type, Int64Type};
use arrow_schema::DataType;

use super::MultiscaleEncoding;
use crate::geometry::{GeometryError, GeometryFamily as GeometryType};

use super::{GEOLOD_COLUMN, MultiscaleLevel};
use crate::geometry::{
  CoordinateSpace, NativeGeometryArrayBuilder, PbfArrayBuilder, QuantizationTransform,
  QuantizedGeometry, WkbArrayBuilder,
};

/// Builds one multiscale level array through its selected physical encoding.
pub(crate) enum MultiscaleLevelArrayBuilder {
  Pbf(PbfArrayBuilder),
  Wkb(WkbArrayBuilder),
  NativeQuantized(NativeGeometryArrayBuilder<Int64Type>),
  Native(NativeGeometryArrayBuilder<Float64Type>),
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
      Self::WkbQuantized => DataType::Binary,
      Self::Wkb => DataType::Binary,
      Self::NativeQuantized => {
        NativeGeometryArrayBuilder::<Int64Type>::data_type(geometry_type, has_z, has_m)
      }
      Self::NativeQuantizedFloat => {
        NativeGeometryArrayBuilder::<Float64Type>::data_type(geometry_type, has_z, has_m)
      }
      Self::Native => {
        NativeGeometryArrayBuilder::<Float64Type>::data_type(geometry_type, has_z, has_m)
      }
    }
  }

  pub(crate) fn missing_component_value(self) -> &'static str {
    match self {
      Self::Pbf => "0",
      Self::WkbQuantized => "0",
      Self::Wkb => "0",
      Self::NativeQuantized => "null",
      Self::NativeQuantizedFloat => "null",
      Self::Native => "null",
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
      Self::Pbf | Self::WkbQuantized | Self::Wkb | Self::NativeQuantizedFloat | Self::Native => {
        Vec::new()
      }
      Self::NativeQuantized => NativeGeometryArrayBuilder::<Int64Type>::coordinate_column_paths(
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

  pub(crate) fn byte_stream_split_column_paths(
    self,
    levels: &[MultiscaleLevel],
    geometry_type: GeometryType,
    has_z: bool,
    has_m: bool,
  ) -> Vec<String> {
    match self {
      Self::NativeQuantizedFloat | Self::Native => {
        NativeGeometryArrayBuilder::<Float64Type>::coordinate_column_paths(
          GEOLOD_COLUMN,
          &levels
            .iter()
            .map(|level| level.column.clone())
            .collect::<Vec<_>>(),
          geometry_type,
          has_z,
          has_m,
        )
      }
      Self::Pbf | Self::WkbQuantized | Self::Wkb | Self::NativeQuantized => Vec::new(),
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
      MultiscaleEncoding::WkbQuantized => {
        MultiscaleLevelArrayBuilder::Wkb(WkbArrayBuilder::new(capacity, CoordinateSpace::Quantized))
      }
      MultiscaleEncoding::Wkb => {
        MultiscaleLevelArrayBuilder::Wkb(WkbArrayBuilder::new(capacity, CoordinateSpace::World))
      }
      MultiscaleEncoding::NativeQuantized => {
        MultiscaleLevelArrayBuilder::NativeQuantized(NativeGeometryArrayBuilder::<Int64Type>::new(
          geometry_type,
          has_z,
          has_m,
          capacity,
          CoordinateSpace::Quantized,
        ))
      }
      MultiscaleEncoding::NativeQuantizedFloat => {
        MultiscaleLevelArrayBuilder::Native(NativeGeometryArrayBuilder::<Float64Type>::new(
          geometry_type,
          has_z,
          has_m,
          capacity,
          CoordinateSpace::Quantized,
        ))
      }
      MultiscaleEncoding::Native => {
        MultiscaleLevelArrayBuilder::Native(NativeGeometryArrayBuilder::<Float64Type>::new(
          geometry_type,
          has_z,
          has_m,
          capacity,
          CoordinateSpace::World,
        ))
      }
    }
  }
}

impl MultiscaleLevelArrayBuilder {
  pub(crate) fn append(
    &mut self,
    geometry: &QuantizedGeometry,
    transform: &QuantizationTransform,
  ) -> Result<(), GeometryError> {
    match self {
      Self::Pbf(writer) => Ok(writer.append(geometry)?),
      Self::Wkb(writer) => Ok(writer.append(geometry, transform)?),
      Self::NativeQuantized(writer) => {
        writer.append_quantized_geometry(geometry, transform);
        Ok(())
      }
      Self::Native(writer) => {
        writer.append_quantized_geometry(geometry, transform);
        Ok(())
      }
    }
  }

  pub(crate) fn append_null(&mut self) {
    match self {
      Self::Pbf(writer) => writer.append_null(),
      Self::Wkb(writer) => writer.append_null(),
      Self::NativeQuantized(builder) => builder.append_null(),
      Self::Native(builder) => builder.append_null(),
    }
  }

  pub(crate) fn finish(self) -> ArrayRef {
    match self {
      Self::Pbf(writer) => writer.finish(),
      Self::Wkb(writer) => writer.finish(),
      Self::NativeQuantized(mut builder) => builder.finish(),
      Self::Native(mut builder) => builder.finish(),
    }
  }
}
