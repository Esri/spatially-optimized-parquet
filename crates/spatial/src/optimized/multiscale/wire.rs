//! Serializes quantized geometry into the compact Esri PBF wire payload.

use anyhow::Result;
use prost::Message;

use super::GeometryEncoding;
use super::payload::FlatGeometryPayload;
#[cfg(test)]
use super::payload::GeometryPayload;
use super::quantize::encode_quantized_payload_into;

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
fn encode_geometry(payload: &GeometryPayload, encoding: &GeometryEncoding) -> Result<Vec<u8>> {
  let mut scratch = GeometryEncodeScratch::default();
  encode_geometry_owned_with_scratch_impl(
    &payload.coordinates,
    &payload.lengths,
    encoding,
    payload.has_z,
    payload.has_m,
    &mut scratch,
  )
}

/// Quantize and encode a flat payload into reusable scratch storage.
///
/// The returned slice remains valid until the scratch value is mutated again.
pub(super) fn encode_flat_geometry_with_scratch<'a>(
  payload: &FlatGeometryPayload,
  encoding: &GeometryEncoding,
  scratch: &'a mut GeometryEncodeScratch,
) -> Result<&'a [u8]> {
  encode_geometry_with_scratch_impl(
    &payload.coordinates,
    &payload.lengths,
    encoding,
    payload.has_z,
    payload.has_m,
    scratch,
  )
}

fn encode_geometry_with_scratch_impl<'a>(
  coordinates: &[crate::geometry::WkbCoordinate],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  has_z: bool,
  has_m: bool,
  scratch: &'a mut GeometryEncodeScratch,
) -> Result<&'a [u8]> {
  let message =
    quantized_message_from_slices(coordinates, lengths, encoding, has_z, has_m, scratch)?;
  scratch.buffer.clear();
  scratch.buffer.reserve(message.encoded_len());
  message.encode(&mut scratch.buffer)?;
  scratch.quantized_coords = message.coords;
  scratch.quantized_lengths = message.lengths;
  Ok(scratch.buffer.as_slice())
}

#[cfg(test)]
fn encode_geometry_owned_with_scratch_impl(
  coordinates: &[crate::geometry::WkbCoordinate],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  has_z: bool,
  has_m: bool,
  scratch: &mut GeometryEncodeScratch,
) -> Result<Vec<u8>> {
  let message =
    quantized_message_from_slices(coordinates, lengths, encoding, has_z, has_m, scratch)?;
  let mut buffer = Vec::with_capacity(message.encoded_len());
  message.encode(&mut buffer)?;
  scratch.quantized_coords = message.coords;
  scratch.quantized_lengths = message.lengths;
  Ok(buffer)
}

fn quantized_message_from_slices(
  coordinates: &[crate::geometry::WkbCoordinate],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  has_z: bool,
  has_m: bool,
  scratch: &mut GeometryEncodeScratch,
) -> Result<PbfGeometry> {
  let mut message = PbfGeometry {
    lengths: std::mem::take(&mut scratch.quantized_lengths),
    coords: std::mem::take(&mut scratch.quantized_coords),
  };
  encode_quantized_payload_into(
    coordinates,
    lengths,
    encoding,
    has_z,
    has_m,
    &mut message.coords,
    &mut message.lengths,
  )?;
  Ok(message)
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
