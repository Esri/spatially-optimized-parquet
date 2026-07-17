//! Encodes quantized geometry with the compact Esri PBF wire format.
//!
//! Field 2 stores part lengths. Field 3 stores signed integer coordinates where x and y use
//! deltas after the first coordinate of each part, while z and m remain absolute. The codec
//! operates on [`QuantizedGeometry`] and therefore does not depend on multiscale level policy.

use anyhow::Result;
use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;
use prost::Message;
use std::sync::Arc;

use super::{QuantizedGeometry, encode_deltas_xy};

/// Reuses quantization vectors and serialization storage across geometry encodes.
#[derive(Debug, Default)]
pub(crate) struct GeometryEncodeScratch {
  quantized_coords: Vec<i64>,
  quantized_lengths: Vec<u32>,
  buffer: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub(crate) struct PbfGeometry {
  #[prost(uint32, repeated, tag = "2")]
  pub(crate) lengths: Vec<u32>,
  #[prost(sint64, repeated, tag = "3")]
  pub(crate) coords: Vec<i64>,
}

/// Decode one Esri PBF geometry payload through the writer's wire schema.
pub(crate) fn decode_pbf_geometry(bytes: &[u8]) -> Result<PbfGeometry> {
  Ok(PbfGeometry::decode(bytes)?)
}

/// Serialize pre-quantized absolute coordinates as a PBF payload.
pub(crate) fn encode_quantized_geometry_with_scratch<'a>(
  geometry: &QuantizedGeometry,
  scratch: &'a mut GeometryEncodeScratch,
) -> Result<&'a [u8]> {
  scratch.quantized_coords.clear();
  scratch
    .quantized_coords
    .extend_from_slice(&geometry.coordinates);
  scratch.quantized_lengths.clear();
  scratch
    .quantized_lengths
    .extend_from_slice(&geometry.lengths);
  encode_deltas_xy(
    &mut scratch.quantized_coords,
    &scratch.quantized_lengths,
    geometry.has_z,
    geometry.has_m,
  );
  let message = PbfGeometry {
    lengths: std::mem::take(&mut scratch.quantized_lengths),
    coords: std::mem::take(&mut scratch.quantized_coords),
  };
  scratch.buffer.clear();
  scratch.buffer.reserve(message.encoded_len());
  message.encode(&mut scratch.buffer)?;
  scratch.quantized_coords = message.coords;
  scratch.quantized_lengths = message.lengths;
  Ok(scratch.buffer.as_slice())
}

/// Builds a BinaryArray containing one Esri PBF payload per geometry row.
pub(crate) struct PbfArrayBuilder {
  builder: BinaryBuilder,
  scratch: GeometryEncodeScratch,
}

impl PbfArrayBuilder {
  /// Create a PBF array builder with capacity for the expected row count.
  pub(crate) fn new(capacity: usize) -> Self {
    Self {
      builder: BinaryBuilder::with_capacity(capacity, capacity * 16),
      scratch: GeometryEncodeScratch::default(),
    }
  }

  /// Append one absolute quantized geometry as a PBF payload.
  pub(crate) fn append(&mut self, geometry: &QuantizedGeometry) -> Result<()> {
    let bytes = encode_quantized_geometry_with_scratch(geometry, &mut self.scratch)?;
    self.builder.append_value(bytes);
    Ok(())
  }

  /// Append a null geometry row.
  pub(crate) fn append_null(&mut self) {
    self.builder.append_null();
  }

  /// Finish the Arrow binary array.
  pub(crate) fn finish(mut self) -> ArrayRef {
    Arc::new(self.builder.finish())
  }
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use super::*;

  #[derive(Clone, PartialEq, Message)]
  struct EsriPbfGeometry {
    #[prost(uint32, repeated, tag = "2")]
    lengths: Vec<u32>,
    #[prost(sint64, repeated, tag = "3")]
    coords: Vec<i64>,
  }

  #[test]
  fn encodes_quantized_geometry() {
    let geometry = QuantizedGeometry {
      coordinates: vec![0, 0, 1, 0, 1, 1, 0, 0],
      lengths: vec![4],
      validity: Default::default(),
      has_z: false,
      has_m: false,
    };
    let mut scratch = GeometryEncodeScratch::default();
    let bytes = encode_quantized_geometry_with_scratch(&geometry, &mut scratch)
      .unwrap()
      .to_vec();
    let decoded = PbfGeometry::decode(Cursor::new(&bytes)).unwrap();
    let esri_decoded = EsriPbfGeometry::decode(Cursor::new(bytes)).unwrap();
    assert_eq!(decoded.lengths, vec![4]);
    assert_eq!(decoded.coords.len(), 8);
    assert_eq!(esri_decoded.lengths, vec![4]);
    assert_eq!(esri_decoded.coords.len(), 8);
  }
}
