//! Serializes quantized geometry into the compact Esri PBF wire payload.

use anyhow::Result;
use prost::Message;

#[cfg(test)]
use super::MultiscaleLevelSpec;
#[cfg(test)]
use super::payload::GeometryPayload;
#[cfg(test)]
use super::quantize::quantize_geometry_into;
use super::quantize::{QuantizedGeometryBuffer, encode_deltas_xy};

/// Reuses quantization vectors and serialization storage across geometry encodes.
#[derive(Debug, Default)]
pub(super) struct GeometryEncodeScratch {
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

/// Quantize and encode a complete geometry payload into a new byte buffer.
#[cfg(test)]
fn encode_geometry(payload: &GeometryPayload, encoding: &MultiscaleLevelSpec) -> Result<Vec<u8>> {
  let mut scratch = GeometryEncodeScratch::default();
  let mut geometry = QuantizedGeometryBuffer::default();
  quantize_geometry_into(
    &payload.coordinates,
    &payload.lengths,
    encoding,
    payload.has_z,
    payload.has_m,
    &mut geometry,
  )?;
  Ok(encode_quantized_geometry_with_scratch(&geometry, &mut scratch)?.to_vec())
}

/// Serialize pre-quantized absolute coordinates as a PBF payload.
pub(super) fn encode_quantized_geometry_with_scratch<'a>(
  geometry: &QuantizedGeometryBuffer,
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

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use geo_types::{Geometry, polygon};

  use super::super::{geometry_payload_from_geometry, levels::QuantizationTransform};
  use super::*;
  use crate::optimized::OptimizedGeometryType;

  #[derive(Clone, PartialEq, Message)]
  struct EsriPbfGeometry {
    #[prost(uint32, repeated, tag = "2")]
    lengths: Vec<u32>,
    #[prost(sint64, repeated, tag = "3")]
    coords: Vec<i64>,
  }

  #[test]
  fn encodes_polygon_pbf() {
    let geometry = Geometry::Polygon(polygon![
        (x: 0.0, y: 0.0),
        (x: 1.0, y: 0.0),
        (x: 1.0, y: 1.0),
        (x: 0.0, y: 0.0),
    ]);
    let payload =
      geometry_payload_from_geometry(&geometry, OptimizedGeometryType::Polygon).unwrap();
    let encoding = MultiscaleLevelSpec {
      level: 0,
      column: "level_0".to_string(),
      resolution: 1.0,
      scale: 1.0,
      transform: QuantizationTransform {
        scale: [1.0, 1.0, 1.0, 1.0],
        translate: [0.0, 0.0, 0.0, 0.0],
      },
      min_length: 3,
    };

    let bytes = encode_geometry(&payload, &encoding).unwrap();
    let decoded = PbfGeometry::decode(Cursor::new(&bytes)).unwrap();
    let esri_decoded = EsriPbfGeometry::decode(Cursor::new(bytes)).unwrap();
    assert_eq!(decoded.lengths, vec![4]);
    assert_eq!(decoded.coords.len(), 8);
    assert_eq!(esri_decoded.lengths, vec![4]);
    assert_eq!(esri_decoded.coords.len(), 8);
  }
}
