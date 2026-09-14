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

//! Builds one multiscale level array through its physical encoding.

use arrow_array::ArrayRef;
use arrow_schema::DataType;

use super::MultiscaleEncoding;
use crate::geometry::{GeometryError, GeometryFamily as GeometryType};

use super::{GEOLOD_COLUMN, MultiscaleLevel};
use crate::geometry::{
  NativeGeometryArrayBuilder, PbfArrayBuilder, QuantizationTransform, QuantizedGeometry,
  WkbArrayBuilder,
};

/// Builds one multiscale level array through its selected physical encoding.
pub(crate) enum MultiscaleLevelArrayBuilder {
  Pbf(PbfArrayBuilder),
  Wkb(WkbArrayBuilder),
  Native(NativeGeometryArrayBuilder),
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
      Self::Wkb => DataType::Binary,
      Self::Native => NativeGeometryArrayBuilder::data_type(geometry_type, has_z, has_m),
    }
  }

  pub(crate) fn missing_component_value(self) -> &'static str {
    match self {
      Self::Pbf => "0",
      Self::Wkb => "0",
      Self::Native => "null",
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
      Self::Native => NativeGeometryArrayBuilder::coordinate_column_paths(
        GEOLOD_COLUMN,
        &levels
          .iter()
          .map(|level| level.column.clone())
          .collect::<Vec<_>>(),
        geometry_type,
        has_z,
        has_m,
      ),
      Self::Pbf | Self::Wkb => Vec::new(),
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
      MultiscaleEncoding::Wkb => MultiscaleLevelArrayBuilder::Wkb(WkbArrayBuilder::new(capacity)),
      MultiscaleEncoding::Native => MultiscaleLevelArrayBuilder::Native(
        NativeGeometryArrayBuilder::new(geometry_type, has_z, has_m, capacity),
      ),
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
      Self::Native(builder) => builder.append_null(),
    }
  }

  pub(crate) fn finish(self) -> ArrayRef {
    match self {
      Self::Pbf(writer) => writer.finish(),
      Self::Wkb(writer) => writer.finish(),
      Self::Native(mut builder) => builder.finish(),
    }
  }
}
