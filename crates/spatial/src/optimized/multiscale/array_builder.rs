//! Builds one multiscale level through the selected physical encoding.

use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;

use crate::optimized::OptimizedGeometryType;
use crate::output::MultiscaleEncoding;

use super::GeometryEncoding;
use super::native::NativeGeometryArrayBuilder;
use super::payload::FlatGeometryPayload;
use super::quantize::{OptionalComponentValidity, quantize_native_geometry_payload_into};
use super::wire::{GeometryEncodeScratch, encode_flat_geometry_with_scratch};

pub(super) enum MultiscaleArrayBuilder {
  Pbf(BinaryBuilder),
  QuantizedNative(NativeGeometryArrayBuilder),
}

#[derive(Default)]
pub(super) struct MultiscaleEncodeScratch {
  pbf: GeometryEncodeScratch,
  quantized_coordinates: Vec<i64>,
  quantized_lengths: Vec<u32>,
  quantized_validity: OptionalComponentValidity,
}

impl MultiscaleArrayBuilder {
  pub(super) fn new(
    encoding: MultiscaleEncoding,
    geometry_type: OptimizedGeometryType,
    has_z: bool,
    has_m: bool,
    capacity: usize,
  ) -> Self {
    match encoding {
      MultiscaleEncoding::Pbf => Self::Pbf(BinaryBuilder::with_capacity(capacity, capacity * 16)),
      MultiscaleEncoding::QuantizedNative => Self::QuantizedNative(
        NativeGeometryArrayBuilder::new(geometry_type, has_z, has_m, capacity),
      ),
    }
  }

  pub(super) fn append_geometry(
    &mut self,
    payload: &FlatGeometryPayload,
    encoding: &GeometryEncoding,
    scratch: &mut MultiscaleEncodeScratch,
  ) -> anyhow::Result<()> {
    match self {
      Self::Pbf(builder) => {
        let encoded = encode_flat_geometry_with_scratch(payload, encoding, &mut scratch.pbf)?;
        builder.append_value(encoded);
      }
      Self::QuantizedNative(builder) => {
        quantize_native_geometry_payload_into(
          &payload.coordinates,
          &payload.lengths,
          encoding,
          payload.has_z,
          payload.has_m,
          &mut scratch.quantized_coordinates,
          &mut scratch.quantized_lengths,
          &mut scratch.quantized_validity,
        )?;
        builder.append_geometry(
          &scratch.quantized_coordinates,
          &scratch.quantized_lengths,
          payload.has_z,
          payload.has_m,
          &scratch.quantized_validity,
        );
      }
    }
    Ok(())
  }

  pub(super) fn append_null(&mut self) {
    match self {
      Self::Pbf(builder) => builder.append_null(),
      Self::QuantizedNative(builder) => builder.append_null(),
    }
  }

  pub(super) fn finish(self) -> ArrayRef {
    match self {
      Self::Pbf(mut builder) => Arc::new(builder.finish()),
      Self::QuantizedNative(mut builder) => builder.finish(),
    }
  }
}
