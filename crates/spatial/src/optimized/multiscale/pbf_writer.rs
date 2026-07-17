//! Writes quantized multiscale geometry as Esri PBF binary arrays.

use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;

use super::quantize::QuantizedGeometryBuffer;
use super::wire::{GeometryEncodeScratch, encode_quantized_geometry_with_scratch};

pub(in crate::optimized) struct PbfGeometryWriter {
  builder: BinaryBuilder,
  scratch: GeometryEncodeScratch,
}

impl PbfGeometryWriter {
  pub(super) fn new(capacity: usize) -> Self {
    Self {
      builder: BinaryBuilder::with_capacity(capacity, capacity * 16),
      scratch: GeometryEncodeScratch::default(),
    }
  }

  pub(super) fn append(&mut self, geometry: &QuantizedGeometryBuffer) -> anyhow::Result<()> {
    let encoded = encode_quantized_geometry_with_scratch(geometry, &mut self.scratch)?;
    self.builder.append_value(encoded);
    Ok(())
  }

  pub(super) fn append_null(&mut self) {
    self.builder.append_null();
  }

  pub(super) fn finish(mut self) -> ArrayRef {
    Arc::new(self.builder.finish())
  }
}
