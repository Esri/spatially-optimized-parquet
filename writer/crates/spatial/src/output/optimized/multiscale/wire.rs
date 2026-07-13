//! Serializes quantized geometry into the compact Esri PBF wire payload.

use anyhow::Result;
use prost::Message;

use super::GeometryEncoding;
use super::payload::{FlatGeometryPayload, GeometryPayload};
use super::quantize::encode_quantized_payload_into;

/// Reuses quantization vectors and serialization storage across geometry encodes.
#[derive(Debug, Default)]
pub struct GeometryEncodeScratch {
  quantized_coords: Vec<i64>,
  quantized_lengths: Vec<u32>,
  buffer: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
struct PbfGeometry {
  #[prost(uint32, repeated, tag = "2")]
  lengths: Vec<u32>,
  #[prost(sint64, repeated, tag = "3")]
  coords: Vec<i64>,
}

/// Quantize and encode a complete geometry payload into a new byte buffer.
pub fn encode_geometry(payload: &GeometryPayload, encoding: &GeometryEncoding) -> Result<Vec<u8>> {
  let mut scratch = GeometryEncodeScratch::default();
  encode_geometry_owned_with_scratch_impl(&payload.coords, &payload.lengths, encoding, &mut scratch)
}

/// Quantize and encode a flat payload into reusable scratch storage.
///
/// The returned slice remains valid until the scratch value is mutated again.
pub fn encode_flat_geometry_with_scratch<'a>(
  payload: &FlatGeometryPayload,
  encoding: &GeometryEncoding,
  scratch: &'a mut GeometryEncodeScratch,
) -> Result<&'a [u8]> {
  encode_geometry_with_scratch_impl(&payload.coords, &payload.lengths, encoding, scratch)
}

/// Quantize with reusable scratch vectors and return an owned encoded buffer.
pub fn encode_flat_geometry_owned_with_scratch(
  payload: &FlatGeometryPayload,
  encoding: &GeometryEncoding,
  scratch: &mut GeometryEncodeScratch,
) -> Result<Vec<u8>> {
  encode_geometry_owned_with_scratch_impl(&payload.coords, &payload.lengths, encoding, scratch)
}

fn encode_geometry_with_scratch_impl<'a>(
  coords: &[f64],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  scratch: &'a mut GeometryEncodeScratch,
) -> Result<&'a [u8]> {
  let message = quantized_message_from_slices(coords, lengths, encoding, scratch)?;
  scratch.buffer.clear();
  scratch.buffer.reserve(message.encoded_len());
  message.encode(&mut scratch.buffer)?;
  scratch.quantized_coords = message.coords;
  scratch.quantized_lengths = message.lengths;
  Ok(scratch.buffer.as_slice())
}

fn encode_geometry_owned_with_scratch_impl(
  coords: &[f64],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  scratch: &mut GeometryEncodeScratch,
) -> Result<Vec<u8>> {
  let message = quantized_message_from_slices(coords, lengths, encoding, scratch)?;
  let mut buffer = Vec::with_capacity(message.encoded_len());
  message.encode(&mut buffer)?;
  scratch.quantized_coords = message.coords;
  scratch.quantized_lengths = message.lengths;
  Ok(buffer)
}

fn quantized_message_from_slices(
  coords: &[f64],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  scratch: &mut GeometryEncodeScratch,
) -> Result<PbfGeometry> {
  let mut message = PbfGeometry {
    lengths: std::mem::take(&mut scratch.quantized_lengths),
    coords: std::mem::take(&mut scratch.quantized_coords),
  };
  encode_quantized_payload_into(
    coords,
    lengths,
    encoding,
    &mut message.coords,
    &mut message.lengths,
  )?;
  Ok(message)
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use geo_types::{Geometry, polygon};

  use super::*;
  use crate::analysis::DisplayGeometryType;
  use crate::metadata::output::QuantizationTransform;
  use crate::output::optimized::multiscale::geometry_payload_from_geometry;

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
    let payload = geometry_payload_from_geometry(&geometry, DisplayGeometryType::Polygon).unwrap();
    let encoding = GeometryEncoding {
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
