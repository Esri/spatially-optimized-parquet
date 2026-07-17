//! Resolves and writes one multiscale level through its physical encoding.

use arrow_array::ArrayRef;
use arrow_schema::DataType;

use crate::optimized::OptimizedGeometryType;
use crate::output::{ESRI_PBF_ENCODING, MultiscaleEncoding, QUANTIZED_NATIVE_ENCODING};

use super::MultiscaleLevelSpec;
use super::native_writer::NativeGeometryArrayBuilder;
use super::native_writer::{native_coordinate_column_paths, native_geometry_data_type};
use super::pbf_writer::PbfGeometryWriter;
use super::quantize::QuantizedGeometryBuffer;

pub(crate) enum QuantizedGeometryWriter {
  Pbf(PbfGeometryWriter),
  QuantizedNative(NativeGeometryArrayBuilder),
}

impl MultiscaleEncoding {
  pub(crate) fn geometry_data_type(
    self,
    geometry_type: OptimizedGeometryType,
    has_z: bool,
    has_m: bool,
  ) -> DataType {
    match self {
      Self::Pbf => DataType::Binary,
      Self::QuantizedNative => native_geometry_data_type(geometry_type, has_z, has_m),
    }
  }

  pub(crate) fn metadata_identifier(self) -> &'static str {
    match self {
      Self::Pbf => ESRI_PBF_ENCODING,
      Self::QuantizedNative => QUANTIZED_NATIVE_ENCODING,
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
    levels: &[MultiscaleLevelSpec],
    geometry_type: OptimizedGeometryType,
    has_z: bool,
    has_m: bool,
  ) -> Vec<String> {
    match self {
      Self::Pbf => Vec::new(),
      Self::QuantizedNative => native_coordinate_column_paths(levels, geometry_type, has_z, has_m),
    }
  }

  pub(crate) fn resolve_writer(
    self,
    geometry_type: OptimizedGeometryType,
    has_z: bool,
    has_m: bool,
    capacity: usize,
  ) -> QuantizedGeometryWriter {
    match self {
      MultiscaleEncoding::Pbf => QuantizedGeometryWriter::Pbf(PbfGeometryWriter::new(capacity)),
      MultiscaleEncoding::QuantizedNative => QuantizedGeometryWriter::QuantizedNative(
        NativeGeometryArrayBuilder::new(geometry_type, has_z, has_m, capacity),
      ),
    }
  }
}

impl QuantizedGeometryWriter {
  pub(crate) fn append(&mut self, geometry: &QuantizedGeometryBuffer) -> anyhow::Result<()> {
    match self {
      Self::Pbf(writer) => writer.append(geometry),
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
