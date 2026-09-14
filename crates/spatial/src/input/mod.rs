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

//! Resolves and opens GeoPackage or Parquet through one explicit source boundary.
//!
//! [`mod@format`] owns source identification while [`source`] owns the shared contract, location
//! classification, and format routing. Parquet sources register HTTP object stores so DataFusion
//! owns ranged reads, decoding, and reusable DataFrame caching.

mod error;
mod format;
mod gpkg;
mod metadata;
pub(crate) mod parquet;
mod source;

pub use error::InputError;
pub use format::SourceFormat;
pub(crate) use metadata::{SourceCoveringMetadata, SourceDatasetMetadata, SourceGeometryMetadata};
pub use source::RowRange;
pub(crate) use source::{InputOpenOptions, InputSource, open_input};
