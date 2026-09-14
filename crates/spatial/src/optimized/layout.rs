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

//! Resolves optimized GeoParquet layout decisions.

use crate::optimized::MultiscaleEncoding;
use crate::optimized::multiscale::MultiscaleLevel;
use crate::pipeline::{OutputOptions, PipelineError, SpatialWriteContext};

use super::{ClusteringFamily, GeometryInfo};

/// Defines clustering and encoding choices for optimized GeoParquet output.
#[derive(Debug, Clone)]
pub(crate) struct OptimizedLayout {
  geometry: GeometryInfo,
  cluster_depth: u32,
  levels: Vec<MultiscaleLevel>,
  multiscale_encoding: MultiscaleEncoding,
  write_sop: bool,
  write_extensions: bool,
}

impl OptimizedLayout {
  /// Resolve optimized layout decisions from prepared GeoParquet data.
  pub(crate) fn new(
    context: &SpatialWriteContext,
    output_options: &OutputOptions,
  ) -> Result<Self, PipelineError> {
    let geometry = GeometryInfo::resolve(context.source());
    if geometry.clustering_family == ClusteringFamily::ComplexGeometry
      && output_options.cluster_depth > 31
    {
      return Err(PipelineError::InvalidRequest(
        "resolve optimized layout: XZ cluster depth must be between 1 and 31".to_string(),
      ));
    }
    let levels = match geometry.family {
      crate::geometry::GeometryFamily::Polyline | crate::geometry::GeometryFamily::Polygon => {
        MultiscaleLevel::create_all(output_options.output_wkid, geometry.family)
      }
      crate::geometry::GeometryFamily::Point | crate::geometry::GeometryFamily::MultiPoint => {
        Vec::new()
      }
    };
    if output_options.write_extensions
      && matches!(
        geometry.family,
        crate::geometry::GeometryFamily::Polyline | crate::geometry::GeometryFamily::Polygon
      )
      && output_options.multiscale_encoding != MultiscaleEncoding::Pbf
    {
      return Err(PipelineError::InvalidRequest(
        "resolve optimized layout: --write-extensions requires PBF multiscale encoding for line and polygon output"
          .to_string(),
      ));
    }
    Ok(Self {
      geometry,
      cluster_depth: output_options.cluster_depth,
      levels,
      multiscale_encoding: output_options.multiscale_encoding,
      write_sop: output_options.write_sop,
      write_extensions: output_options.write_extensions,
    })
  }

  /// Return geometry facts that select the clustering strategy.
  pub(crate) fn geometry(&self) -> &GeometryInfo {
    &self.geometry
  }

  /// Return the Z bit width or XZ maximum level used by this layout.
  pub(crate) fn cluster_depth(&self) -> u32 {
    self.cluster_depth
  }

  /// Return multiscale levels for complex geometry output.
  pub(crate) fn levels(&self) -> &[MultiscaleLevel] {
    &self.levels
  }

  /// Return the selected multiscale payload encoding.
  pub(crate) fn multiscale_encoding(&self) -> MultiscaleEncoding {
    self.multiscale_encoding
  }

  /// Return whether this layout writes multiscale level-of-detail columns.
  pub(crate) fn writes_lod_columns(&self) -> bool {
    matches!(
      self.geometry.family,
      crate::geometry::GeometryFamily::Polyline | crate::geometry::GeometryFamily::Polygon
    )
  }

  /// Return whether this layout writes extension level-of-detail metadata.
  pub(crate) fn writes_extension_lod(&self) -> bool {
    self.writes_lod_columns()
  }

  /// Return whether this layout emits SOP `geodisplay` metadata.
  pub(crate) fn writes_sop(&self) -> bool {
    self.write_sop
  }

  /// Return whether this layout emits draft GeoParquet extension metadata.
  pub(crate) fn writes_extensions(&self) -> bool {
    self.write_extensions
  }

  /// Return floating-point payload columns that require byte-stream-split encoding.
  pub(crate) fn byte_stream_split_column_paths(&self) -> Vec<String> {
    self.multiscale_encoding.byte_stream_split_column_paths(
      &self.levels,
      self.geometry.family,
      self.geometry.has_z,
      self.geometry.has_m,
    )
  }
}
