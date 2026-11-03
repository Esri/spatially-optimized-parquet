// Copyright 2026 Esri
//
// Licensed under the Apache License, Version 2.0 (the "License");
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

//! Adapts owned, replayable Arrow batches to the spatial pipeline.

use std::sync::Arc;

use arrow_array::RecordBatch;
use arrow_schema::{DataType, SchemaRef};
use datafusion::{dataframe::DataFrame, datasource::MemTable, execution::context::SessionContext};
use futures_util::future::BoxFuture;

use super::{InputError, InputSource, RowRange, SourceDatasetMetadata, SourceGeometryMetadata};
use crate::geometry::{
  CoordinateDimensions, GeometryArray, GeometryColumn, GeometryEncoding, GeometryFamily,
  GeometryType, WkbHeader,
};
use crate::geoparquet::SpatialReference;

/// Declare the WKB column, geometry kinds, dimensions and CRS of Arrow input, including null rows.
#[derive(Debug, Clone)]
pub struct GeometryDeclaration {
  column: String,
  geometry_types: Vec<GeometryType>,
  spatial_reference: SpatialReference,
  dimensions: CoordinateDimensions,
}

impl GeometryDeclaration {
  /// Create an XY WKB declaration with an EPSG CRS and one compatible geometry family.
  ///
  /// Reject empty column names, empty or incompatible geometry kinds, and invalid EPSG codes.
  pub fn wkb(
    column: impl Into<String>,
    geometry_types: Vec<GeometryType>,
    wkid: u32,
  ) -> Result<Self, InputError> {
    let column = column.into();
    if column.is_empty() {
      return Err(InputError::Metadata(
        "geometry column must not be empty".into(),
      ));
    }
    GeometryFamily::from_types(&geometry_types)?;
    Ok(Self {
      column,
      geometry_types,
      spatial_reference: SpatialReference::from_epsg(wkid)?,
      dimensions: CoordinateDimensions::Xy,
    })
  }

  /// Set the dimensions required of every non-null WKB value.
  pub fn dimensions(mut self, dimensions: CoordinateDimensions) -> Self {
    self.dimensions = dimensions;
    self
  }
}

/// Retain validated batches for repeated metadata and writer scans without an intermediate file.
pub(super) struct ArrowInputSource {
  schema: SchemaRef,
  batches: Vec<RecordBatch>,
  metadata: SourceDatasetMetadata,
  row_count: u64,
}

impl ArrowInputSource {
  /// Validate schema and geometry declarations before exposing a replayable source.
  pub(super) fn new(
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    geometry: GeometryDeclaration,
  ) -> Result<Self, InputError> {
    let field = schema
      .field_with_name(&geometry.column)
      .map_err(|source| InputError::Arrow {
        operation: "find declared geometry column",
        source,
      })?;
    if !matches!(
      field.data_type(),
      DataType::Binary | DataType::LargeBinary | DataType::BinaryView
    ) {
      return Err(InputError::Metadata(
        "Arrow WKB geometry requires a binary column".into(),
      ));
    }
    let mut row_count = 0_u64;
    for batch in &batches {
      if batch.schema() != schema {
        return Err(InputError::Metadata(
          "Arrow batch schema does not match the input schema".into(),
        ));
      }
      row_count = row_count
        .checked_add(
          u64::try_from(batch.num_rows())
            .map_err(|_| InputError::Metadata("Arrow row count exceeds u64".into()))?,
        )
        .ok_or_else(|| InputError::Metadata("Arrow row count exceeds u64".into()))?;
      let column = batch
        .column_by_name(&geometry.column)
        .expect("validated batch schema contains the geometry column");
      let values =
        GeometryArray::try_new(column.as_ref()).map_err(|source| InputError::DataFusion {
          operation: "read Arrow geometry values",
          source,
        })?;
      for value in values.values().flatten() {
        let header = WkbHeader::read(value)?;
        if !geometry.geometry_types.contains(&header.ty) || header.dimensions != geometry.dimensions
        {
          return Err(InputError::Metadata(
            "Arrow WKB value does not match its geometry declaration".into(),
          ));
        }
      }
    }

    // Publish only schema-compatible batches and matching geometry metadata.
    let GeometryDeclaration {
      column,
      geometry_types,
      spatial_reference,
      dimensions,
    } = geometry;
    let (has_z, has_m) = match dimensions {
      CoordinateDimensions::Xy => (false, false),
      CoordinateDimensions::Xyz => (true, false),
      CoordinateDimensions::Xym => (false, true),
      CoordinateDimensions::Xyzm => (true, true),
    };
    Ok(Self {
      schema,
      batches,
      row_count,
      metadata: SourceDatasetMetadata {
        geometry: Some(SourceGeometryMetadata {
          column,
          encoding: GeometryEncoding::Wkb,
          geometry_types,
          bbox: None,
          covering: None,
          projjson: spatial_reference.projjson,
          has_z,
          has_m,
        }),
        passthrough_kv: Vec::new(),
      },
    })
  }
}

impl InputSource for ArrowInputSource {
  /// Get the validated Arrow schema.
  fn schema(&self) -> Result<SchemaRef, InputError> {
    Ok(Arc::clone(&self.schema))
  }

  /// Get the number of retained rows, including null geometry.
  fn total_rows(&self) -> Result<u64, InputError> {
    Ok(self.row_count)
  }

  /// Get the declared primary WKB column.
  fn inferred_geometry_column(&self) -> Result<Option<GeometryColumn>, InputError> {
    Ok(
      self
        .metadata
        .geometry
        .as_ref()
        .map(|geometry| -> GeometryColumn {
          GeometryColumn {
            column: geometry.column.clone(),
            encoding: geometry.encoding,
            ty: if geometry.geometry_types.len() == 1 {
              Some(geometry.geometry_types[0])
            } else {
              None
            },
          }
        }),
    )
  }

  /// Get declared geometry metadata without inventing an observed extent.
  fn source_metadata(&self) -> Result<SourceDatasetMetadata, InputError> {
    Ok(self.metadata.clone())
  }

  /// Create a replayable single-partition scan preserving batch and row order before sorting.
  fn to_dataframe<'a>(
    &'a self,
    context: &'a SessionContext,
    row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame, InputError>> {
    Box::pin(async move {
      let table = MemTable::try_new(Arc::clone(&self.schema), vec![self.batches.clone()]).map_err(
        |source| InputError::DataFusion {
          operation: "create Arrow input table",
          source,
        },
      )?;
      let dataframe =
        context
          .read_table(Arc::new(table))
          .map_err(|source| InputError::DataFusion {
            operation: "read Arrow input table",
            source,
          })?;
      dataframe
        .limit(row_range.start(), row_range.num())
        .map_err(|source| InputError::DataFusion {
          operation: "select Arrow input rows",
          source,
        })
    })
  }
}
