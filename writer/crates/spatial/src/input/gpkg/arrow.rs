use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::RecordBatch;
use arrow_array::RecordBatchReader;
use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_schema::{Field, Schema, SchemaRef};
use datafusion::common::DataFusionError;
use futures_util::stream;
use gdal::ArrowArrayStream;
use gdal::cpl::CslStringList;
use gdal::vector::{LayerAccess, OwnedLayer};

use super::open::open_gpkg_layer;

const GEOMETRY_EXTENSION_NAME: &str = "ARROW:extension:name";
const GEOMETRY_EXTENSION_VALUE: &str = "ogc.wkb";

/// Owns the GDAL layer and Arrow reader that share one stream lifetime.
struct GpkgArrowReader {
  _layer: OwnedLayer,
  reader: ArrowArrayStreamReader,
}

/// Owns one GDAL Arrow stream and its batch normalization state.
pub(super) struct GpkgBatchState {
  arrow_reader: GpkgArrowReader,
  schema: SchemaRef,
  remaining: Option<usize>,
}

/// Load the normalized Arrow schema without consuming feature batches.
pub(super) fn load_schema(
  path: &Path,
  layer_name: &str,
  geometry_column: &str,
) -> Result<SchemaRef> {
  let arrow_reader = open_arrow_reader(path, layer_name, None)?;
  Ok(normalize_schema(
    arrow_reader.reader.schema(),
    geometry_column,
  ))
}

/// Open a GDAL Arrow stream while retaining the layer that owns its lifetime.
fn open_arrow_reader(
  path: &Path,
  layer_name: &str,
  attribute_filter: Option<&str>,
) -> Result<GpkgArrowReader> {
  let mut layer = open_gpkg_layer(path, layer_name)?;
  if let Some(attribute_filter) = attribute_filter {
    layer
      .set_attribute_filter(attribute_filter)
      .with_context(|| {
        format!("failed to apply GeoPackage attribute filter {attribute_filter:?}")
      })?;
  }
  let options = CslStringList::from_iter(["INCLUDE_FID=NO", "GEOMETRY_ENCODING=WKB"]);
  let mut stream = FFI_ArrowArrayStream::empty();
  unsafe {
    layer.read_arrow_stream(
      (&mut stream as *mut FFI_ArrowArrayStream).cast::<ArrowArrayStream>(),
      &options,
    )
  }
  .with_context(|| format!("failed to open Arrow stream for GeoPackage layer {layer_name}"))?;
  let reader = ArrowArrayStreamReader::try_new(stream)
    .with_context(|| format!("failed to create Arrow reader for GeoPackage layer {layer_name}"))?;
  Ok(GpkgArrowReader {
    _layer: layer,
    reader,
  })
}

/// Retain the GDAL layer owner alongside its Arrow reader for the stream lifetime.
pub(super) fn open_gpkg_batch_state(
  input_path: &Path,
  layer_name: &str,
  schema: SchemaRef,
  attribute_filter: Option<&str>,
  limit: Option<usize>,
) -> Result<GpkgBatchState> {
  Ok(GpkgBatchState {
    arrow_reader: open_arrow_reader(input_path, layer_name, attribute_filter)?,
    schema,
    remaining: limit,
  })
}

/// Convert a stateful GDAL Arrow reader into a fallible asynchronous batch stream.
pub(super) fn gpkg_batch_stream(
  state: GpkgBatchState,
) -> impl futures_util::Stream<Item = Result<RecordBatch>> + Send + 'static {
  stream::unfold(Some(state), |state| async move {
    let mut state = match state {
      Some(state) => state,
      None => return None,
    };
    if state.remaining == Some(0) {
      return None;
    }

    match state.arrow_reader.reader.next() {
      Some(Ok(batch)) => {
        let batch = match normalize_batch_schema(batch, state.schema.clone()) {
          Ok(batch) => batch,
          Err(err) => return Some((Err(err), None)),
        };
        let batch = truncate_batch(batch, &mut state.remaining);
        Some((Ok(batch), Some(state)))
      }
      Some(Err(err)) => Some((Err(err.into()), None)),
      None => None,
    }
  })
}

pub(super) fn to_datafusion_error(err: anyhow::Error) -> DataFusionError {
  DataFusionError::External(err.into())
}

/// Normalize provider-specific geometry field names and extension metadata.
fn normalize_schema(schema: SchemaRef, geometry_column: &str) -> SchemaRef {
  let Some(source_geometry_name) = find_geometry_field_name(schema.as_ref()) else {
    return schema;
  };
  if source_geometry_name == geometry_column {
    return schema;
  }

  let fields: Vec<_> = schema
    .fields()
    .iter()
    .map(|field| {
      if field.name() == source_geometry_name {
        Arc::new(
          Field::new(
            geometry_column,
            field.data_type().clone(),
            field.is_nullable(),
          )
          .with_metadata(field.metadata().clone()),
        )
      } else {
        field.clone()
      }
    })
    .collect();

  Arc::new(Schema::new_with_metadata(fields, schema.metadata().clone()))
}

fn find_geometry_field_name(schema: &Schema) -> Option<&str> {
  schema.fields().iter().find_map(|field| {
    field
      .metadata()
      .get(GEOMETRY_EXTENSION_NAME)
      .filter(|value| value.as_str() == GEOMETRY_EXTENSION_VALUE)
      .map(|_| field.name().as_str())
  })
}

/// Replace a provider batch schema with the stable source schema after field normalization.
fn normalize_batch_schema(batch: RecordBatch, schema: SchemaRef) -> Result<RecordBatch> {
  if batch.schema() == schema {
    return Ok(batch);
  }

  Ok(RecordBatch::try_new(schema, batch.columns().to_vec())?)
}

/// Slice a batch to the remaining requested row count and update that count.
fn truncate_batch(batch: RecordBatch, remaining: &mut Option<usize>) -> RecordBatch {
  let Some(remaining_rows) = remaining else {
    return batch;
  };

  if batch.num_rows() <= *remaining_rows {
    *remaining_rows -= batch.num_rows();
    return batch;
  }

  let sliced = batch.slice(0, *remaining_rows);
  *remaining_rows = 0;
  sliced
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;
  use std::sync::Arc;

  use arrow_array::{BinaryArray, Int32Array, RecordBatch};
  use arrow_schema::{DataType, Field, Schema};

  use super::{
    GEOMETRY_EXTENSION_NAME, GEOMETRY_EXTENSION_VALUE, normalize_batch_schema, normalize_schema,
    truncate_batch,
  };

  fn provider_schema(geometry_name: &str) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
      Field::new("id", DataType::Int32, false),
      Field::new(geometry_name, DataType::Binary, true).with_metadata(HashMap::from([(
        GEOMETRY_EXTENSION_NAME.to_string(),
        GEOMETRY_EXTENSION_VALUE.to_string(),
      )])),
    ]))
  }

  #[test]
  fn schema_normalization_renames_only_the_geometry_extension_field() {
    let schema = normalize_schema(provider_schema("geom"), "geometry");

    assert_eq!(schema.fields()[0].name(), "id");
    assert_eq!(schema.fields()[1].name(), "geometry");
    assert_eq!(
      schema.fields()[1].metadata().get(GEOMETRY_EXTENSION_NAME),
      Some(&GEOMETRY_EXTENSION_VALUE.to_string())
    );
  }

  #[test]
  fn schema_normalization_preserves_schema_without_geometry_extension() {
    let schema = Arc::new(Schema::new(vec![Field::new(
      "payload",
      DataType::Binary,
      true,
    )]));

    assert!(Arc::ptr_eq(
      &normalize_schema(schema.clone(), "geometry"),
      &schema
    ));
  }

  #[test]
  fn batch_normalization_applies_the_stable_schema_without_reordering_columns() {
    let provider_schema = provider_schema("geom");
    let stable_schema = normalize_schema(provider_schema.clone(), "geometry");
    let batch = RecordBatch::try_new(
      provider_schema,
      vec![
        Arc::new(Int32Array::from(vec![1, 2])),
        Arc::new(BinaryArray::from(vec![Some(&[1u8][..]), None])),
      ],
    )
    .unwrap();

    let normalized = normalize_batch_schema(batch, stable_schema.clone()).unwrap();

    assert_eq!(normalized.schema(), stable_schema);
    assert_eq!(normalized.num_rows(), 2);
  }

  #[test]
  fn batch_truncation_consumes_only_the_remaining_rows() {
    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));
    let batch =
      RecordBatch::try_new(schema, vec![Arc::new(Int32Array::from(vec![1, 2, 3]))]).unwrap();
    let mut remaining = Some(2);

    let truncated = truncate_batch(batch, &mut remaining);

    assert_eq!(truncated.num_rows(), 2);
    assert_eq!(remaining, Some(0));
  }
}
