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

//! Owns the GeoParquet product contract and plain output computations.

mod covering;
mod error;
mod extensions;
mod geo_metadata;
mod spatial_reference;

pub(crate) use covering::{COVERING_BBOX_COLUMN, bbox_field_expr, geometry_bbox_expr};
pub use error::GeoParquetError;
pub(crate) use extensions::{
  LodEncoding, LodLevel, LodMetadata, LodTransform, OrderingMetadata, XzOrderingMetadata,
  ZOrderingMetadata,
};
pub(crate) use geo_metadata::{GeoMetadata, GeoMetadataInput};
pub use spatial_reference::DEFAULT_OUTPUT_WKID;
pub(crate) use spatial_reference::{
  SpatialReference, WEB_MERCATOR_MAX_COORDINATE, WEB_MERCATOR_OUTPUT_WKID, WEB_MERCATOR_WORLD_WIDTH,
};

#[cfg(test)]
mod tests;
