// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Encodes quantized geometry with the compact Esri PBF wire format.
//!
//! Field 2 stores part lengths. Field 3 stores signed integer coordinates where x and y use
//! deltas after the first coordinate of each part, while z and m remain absolute. The codec
//! operates on [`QuantizedGeometry`] and therefore does not depend on multiscale level policy.

use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;
use prost::Message;
use std::sync::Arc;

use super::{GeometryError, QuantizedGeometry};

type Result<T> = std::result::Result<T, GeometryError>;

#[derive(Debug, Default)]
struct GeometryEncodeScratch {
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

impl PbfGeometry {
  /// Decode one Esri PBF geometry payload through the writer's wire schema.
  pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self> {
    Self::decode(bytes).map_err(|error| GeometryError::Pbf(error.to_string()))
  }
}

impl GeometryEncodeScratch {
  /// Serialize pre-quantized absolute coordinates as a PBF payload.
  fn encode(&mut self, geometry: &QuantizedGeometry) -> Result<&[u8]> {
    self.quantized_coords.clear();
    self
      .quantized_coords
      .extend_from_slice(&geometry.coordinates);
    self.quantized_lengths.clear();
    self.quantized_lengths.extend_from_slice(&geometry.lengths);
    encode_deltas_xy(
      &mut self.quantized_coords,
      &self.quantized_lengths,
      geometry.has_z,
      geometry.has_m,
    );
    let message = PbfGeometry {
      lengths: std::mem::take(&mut self.quantized_lengths),
      coords: std::mem::take(&mut self.quantized_coords),
    };
    self.buffer.clear();
    self.buffer.reserve(message.encoded_len());
    message
      .encode(&mut self.buffer)
      .map_err(|error| GeometryError::Pbf(error.to_string()))?;
    self.quantized_coords = message.coords;
    self.quantized_lengths = message.lengths;
    Ok(self.buffer.as_slice())
  }
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
    let bytes = self.scratch.encode(geometry)?;
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

/// Delta encode XY within each part, leaving Z/M absolute per spec/display-optimization.md, Encoding.
fn encode_deltas_xy(coords: &mut [i64], lengths: &[u32], has_z: bool, has_m: bool) {
  let stride = 2 + usize::from(has_z) + usize::from(has_m);
  let mut offset = 0;

  for &length in lengths {
    let part_end = offset + length as usize * stride;
    let mut prev_x = 0;
    let mut prev_y = 0;

    for (index, vertex) in coords[offset..part_end]
      .chunks_exact_mut(stride)
      .enumerate()
    {
      let x = vertex[0];
      let y = vertex[1];
      if index > 0 {
        vertex[0] = x - prev_x;
        vertex[1] = y - prev_y;
      }
      prev_x = x;
      prev_y = y;
    }

    offset = part_end;
  }
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use super::*;
  use crate::geometry::{
    Coord, Geometry, GeometryType, QuantizationOptions, QuantizationTransform, quantize_into,
  };

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
      ty: crate::geometry::GeometryType::LineString,
      coordinates: vec![0, 0, 1, 0, 1, 1, 0, 0],
      lengths: vec![4],
      polygon_ring_counts: Vec::new(),
      has_z: false,
      has_m: false,
    };
    let mut scratch = GeometryEncodeScratch::default();
    let bytes = scratch.encode(&geometry).unwrap().to_vec();
    let decoded = PbfGeometry::decode(Cursor::new(&bytes)).unwrap();
    let esri_decoded = EsriPbfGeometry::decode(Cursor::new(bytes)).unwrap();
    assert_eq!(decoded.lengths, vec![4]);
    assert_eq!(decoded.coords.len(), 8);
    assert_eq!(esri_decoded.lengths, vec![4]);
    assert_eq!(esri_decoded.coords.len(), 8);
  }

  #[test]
  fn quantizes_xy_from_xyz_input() {
    let geometry = Geometry::new(
      GeometryType::LineString,
      vec![
        Coord {
          x: 1.0,
          y: 2.0,
          z: Some(30.0),
          m: None,
        },
        Coord {
          x: 4.0,
          y: 5.0,
          z: Some(60.0),
          m: None,
        },
      ],
      vec![2],
      Vec::new(),
    )
    .unwrap();
    let options = QuantizationOptions {
      transform: QuantizationTransform {
        scale: [1.0; 4],
        translate: [0.0; 4],
      },
      tolerance: 1.0,
      min_length: 2,
      has_z: false,
      has_m: false,
    };
    let mut quantized = QuantizedGeometry::default();

    quantize_into(&geometry, &options, &mut quantized).unwrap();

    assert_eq!(quantized.lengths, [2]);
    assert_eq!(quantized.coordinates, [1, 2, 4, 5]);
    assert!(!quantized.has_z);
    assert!(!quantized.has_m);

    let mut scratch = GeometryEncodeScratch::default();
    let decoded = PbfGeometry::from_bytes(scratch.encode(&quantized).unwrap()).unwrap();

    assert_eq!(decoded.lengths, [2]);
    assert_eq!(decoded.coords, [1, 2, 3, 3]);
    assert_eq!(quantized.coordinates, [1, 2, 4, 5]);
  }

  mod delta {
    use super::encode_deltas_xy;

    #[test]
    fn resets_xy_deltas_at_each_part() {
      let mut coordinates = vec![10, 20, 12, 23, 100, 200, 95, 190];

      encode_deltas_xy(&mut coordinates, &[2, 2], false, false);

      assert_eq!(coordinates, [10, 20, 2, 3, 100, 200, -5, -10]);
    }

    #[test]
    fn preserves_z_while_delta_encoding_xy() {
      let mut coordinates = vec![10, 20, 100, 12, 23, 90];

      encode_deltas_xy(&mut coordinates, &[2], true, false);

      assert_eq!(coordinates, [10, 20, 100, 2, 3, 90]);
    }

    #[test]
    fn preserves_m_while_delta_encoding_xy() {
      let mut coordinates = vec![10, 20, 1000, 12, 23, 900];

      encode_deltas_xy(&mut coordinates, &[2], false, true);

      assert_eq!(coordinates, [10, 20, 1000, 2, 3, 900]);
    }

    #[test]
    fn preserves_zm_while_delta_encoding_xy() {
      let mut coordinates = vec![10, 20, 100, 1000, 12, 23, 90, 900];

      encode_deltas_xy(&mut coordinates, &[2], true, true);

      assert_eq!(coordinates, [10, 20, 100, 1000, 2, 3, 90, 900]);
    }

    #[test]
    fn skips_empty_parts_and_preserves_single_vertex_parts() {
      let mut coordinates = vec![10, 20, 100, 200];

      encode_deltas_xy(&mut coordinates, &[0, 1, 0, 1], false, false);

      assert_eq!(coordinates, [10, 20, 100, 200]);
    }

    #[test]
    fn accepts_empty_coordinates_and_parts() {
      let mut coordinates = Vec::new();

      encode_deltas_xy(&mut coordinates, &[], true, true);

      assert!(coordinates.is_empty());
    }
  }
}
