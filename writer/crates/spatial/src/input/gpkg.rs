//! Integrates GeoPackage vector layers through GDAL's Arrow C stream interface.
//!
//! [`GpkgInputProvider`] recognizes local `.gpkg` files, opens only the GDAL `GPKG` driver,
//! inventories vector layers, resolves the requested layer, and normalizes geometry type,
//! dimensions, extent, CRS, and Arrow schema. Generic GDAL geometry declarations trigger a
//! bounded feature sample so downstream analysis receives a useful concrete type when possible.
//!
//! GeoPackage does not use DataFusion's native file readers. [`GpkgInputSource::to_dataframe`]
//! calculates rowid boundaries with SQLite offset queries, creates one partition stream
//! per range, and wraps those streams in a DataFusion `StreamingTable`. DataFusion schedules the
//! partitions and downstream operators, while GDAL still owns SQLite access, feature decoding,
//! WKB production, and Arrow conversion.
//!
//! Partition planning and stream creation repeat whenever a job executes a new source DataFrame.
//! Metadata analysis, multi-file range estimation, and final output can therefore reopen and scan
//! the same GeoPackage independently when the job cannot use metadata fast paths.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use arrow_array::RecordBatch;
use arrow_array::RecordBatchReader;
use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_schema::{Field, Schema, SchemaRef};
use datafusion::catalog::streaming::StreamingTable;
use datafusion::common::DataFusionError;
use datafusion::execution::context::TaskContext;
use datafusion::physical_plan::SendableRecordBatchStream;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::streaming::PartitionStream;
use engine::session::configured_target_partitions;
use engine::{DataFrame, SessionContext};
use futures_util::future::BoxFuture;
use futures_util::stream;
use futures_util::{StreamExt, TryStreamExt};
use gdal::ArrowArrayStream;
use gdal::cpl::CslStringList;
use gdal::spatial_ref::SpatialRef;
use gdal::vector::{LayerAccess, OwnedLayer, geometry_type_to_name, sql};
use gdal::{Dataset, DatasetOptions, GdalOpenFlags};
use gdal_sys::OGRwkbGeometryType;

use crate::analysis::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryKind, GeometrySpec};
use crate::input::{InputBatchStream, InputOpenOptions, InputProvider, InputSource, RowRange};
use crate::metadata::source::{SourceDatasetMetadata, SourceGeometryMetadata};

const GEOMETRY_EXTENSION_NAME: &str = "ARROW:extension:name";
const GEOMETRY_EXTENSION_VALUE: &str = "ogc.wkb";
const MAX_GEOMETRY_TYPE_SAMPLE_FEATURES: usize = 64;
const GPKG_ALLOWED_DRIVERS: [&str; 1] = ["GPKG"];
const GPKG_OPEN_OPTIONS: [&str; 2] = ["NOLOCK=YES", "IMMUTABLE=YES"];

#[derive(Debug, Default)]
/// Detects local `.gpkg` files and opens one selected vector layer.
pub struct GpkgInputProvider;

#[derive(Debug, Clone)]
/// Stores normalized GeoPackage metadata and constructs GDAL-backed batch streams.
pub struct GpkgInputSource {
  input_path: PathBuf,
  source_location: String,
  layer_name: String,
  schema: SchemaRef,
  total_rows: u64,
  geometry_spec: Option<GeometrySpec>,
  source_metadata: SourceDatasetMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Summarizes one layer for selection errors and diagnostics.
struct GpkgLayerSummary {
  name: String,
  geometry_type: String,
  feature_count: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Defines an inclusive lower and exclusive upper rowid range for one scan partition.
struct GpkgScanPartition {
  lower_rowid: Option<i64>,
  upper_rowid: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Captures whether sampled features establish one geometry type or mixed content.
enum SampledGeometryType {
  Concrete {
    geometry_kind: GeometryKind,
    has_z: bool,
    has_m: bool,
  },
  Mixed,
}

impl GpkgScanPartition {
  fn attribute_filter(&self) -> Option<String> {
    match (self.lower_rowid, self.upper_rowid) {
      (None, None) => None,
      (Some(lower_rowid), None) => Some(format!("rowid >= {lower_rowid}")),
      (None, Some(upper_rowid)) => Some(format!("rowid < {upper_rowid}")),
      (Some(lower_rowid), Some(upper_rowid)) => {
        Some(format!("rowid >= {lower_rowid} AND rowid < {upper_rowid}"))
      }
    }
  }
}

impl GpkgInputProvider {
  /// Build the stateless GeoPackage provider.
  pub fn new() -> Self {
    Self
  }
}

impl InputProvider for GpkgInputProvider {
  fn name(&self) -> &'static str {
    "GeoPackage"
  }

  fn open<'a>(
    &'a self,
    options: &'a InputOpenOptions,
  ) -> BoxFuture<'a, Result<Option<Arc<dyn InputSource>>>> {
    Box::pin(async move {
      let Some(path) = options.local_path() else {
        return Ok(None);
      };
      if !is_gpkg_path(path) {
        return Ok(None);
      }

      let dataset = open_gpkg_dataset(path)?;
      let layer_summaries = collect_layer_summaries(&dataset)?;
      let layer_name = select_layer_name(options, &layer_summaries)?;
      let mut layer = dataset
        .layer_by_name(&layer_name)
        .with_context(|| format!("failed to open GeoPackage layer {layer_name}"))?;
      let geometry_metadata = build_geometry_metadata(&mut layer, &layer_name)?;
      let schema = load_schema(path, &layer_name, &geometry_metadata.column)?;
      let total_rows = layer
        .try_feature_count()
        .unwrap_or_else(|| layer.feature_count());
      let geometry_kind = geometry_metadata.geometry_types.first().copied();
      let geometry_spec = Some(GeometrySpec {
        column: geometry_metadata.column.clone(),
        encoding: GeometryEncoding::Wkb,
        geometry_kind,
      });
      let source_metadata = SourceDatasetMetadata {
        geometry: Some(geometry_metadata),
        passthrough_kv: Vec::new(),
      };

      Ok(Some(Arc::new(GpkgInputSource {
        input_path: path.to_path_buf(),
        source_location: options.location.clone(),
        layer_name,
        schema,
        total_rows,
        geometry_spec,
        source_metadata,
      }) as Arc<dyn InputSource>))
    })
  }
}

impl InputSource for GpkgInputSource {
  fn format_name(&self) -> &'static str {
    "GeoPackage"
  }

  fn source_location(&self) -> &str {
    &self.source_location
  }

  fn schema(&self) -> Result<SchemaRef> {
    Ok(self.schema.clone())
  }

  fn total_rows(&self) -> Result<u64> {
    Ok(self.total_rows)
  }

  fn inferred_geometry_spec(&self) -> Result<Option<GeometrySpec>> {
    Ok(self.geometry_spec.clone())
  }

  fn source_metadata(&self) -> Result<SourceDatasetMetadata> {
    Ok(self.source_metadata.clone())
  }

  fn read_batches(&self, row_range: RowRange) -> BoxFuture<'_, Result<InputBatchStream>> {
    let input_path = self.input_path.clone();
    let layer_name = self.layer_name.clone();
    let schema = self.schema.clone();
    Box::pin(async move {
      let rows_to_read = row_range.num.map(|num| num.saturating_add(row_range.start));
      let state = open_gpkg_batch_state(&input_path, &layer_name, schema, None, rows_to_read)
        .with_context(|| format!("failed to stream GeoPackage layer {layer_name}"))?;
      let mut rows_to_skip = row_range.start;
      let stream = gpkg_batch_stream(state).filter_map(move |batch| {
        let out = match batch {
          Ok(batch) if rows_to_skip >= batch.num_rows() => {
            rows_to_skip -= batch.num_rows();
            None
          }
          Ok(batch) => {
            let offset = rows_to_skip;
            rows_to_skip = 0;
            Some(Ok(batch.slice(offset, batch.num_rows() - offset)))
          }
          Err(err) => Some(Err(err)),
        };
        std::future::ready(out)
      });
      Ok(Box::pin(stream) as InputBatchStream)
    })
  }

  fn to_dataframe<'a>(
    &'a self,
    ctx: &'a SessionContext,
    row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>> {
    let input_path = self.input_path.clone();
    let layer_name = self.layer_name.clone();
    let schema = self.schema.clone();
    let total_rows = self.total_rows;
    Box::pin(async move {
      let partitions = plan_gpkg_scan_partitions(
        &input_path,
        &layer_name,
        total_rows,
        row_range,
        configured_target_partitions(),
      )?;
      let streams: Vec<_> = partitions
        .into_iter()
        .map(|partition| {
          Arc::new(GpkgPartitionStream {
            input_path: input_path.clone(),
            layer_name: layer_name.clone(),
            schema: schema.clone(),
            attribute_filter: partition.attribute_filter(),
          }) as Arc<dyn PartitionStream>
        })
        .collect();
      let table = StreamingTable::try_new(schema, streams)?;
      let mut df = ctx.read_table(Arc::new(table))?;
      if let Some(num) = row_range.num {
        df = df.limit(0, Some(num))?;
      }
      Ok(df)
    })
  }
}

/// Owns the layer and reader state required to continue one GDAL Arrow stream.
struct GpkgBatchState {
  _layer: OwnedLayer,
  reader: ArrowArrayStreamReader,
  schema: SchemaRef,
  remaining: Option<usize>,
}

#[derive(Debug)]
/// Opens and streams one independently executable GeoPackage rowid partition.
struct GpkgPartitionStream {
  input_path: PathBuf,
  layer_name: String,
  schema: SchemaRef,
  attribute_filter: Option<String>,
}

impl PartitionStream for GpkgPartitionStream {
  fn schema(&self) -> &SchemaRef {
    &self.schema
  }

  fn execute(&self, _ctx: Arc<TaskContext>) -> SendableRecordBatchStream {
    let result = open_gpkg_batch_state(
      &self.input_path,
      &self.layer_name,
      self.schema.clone(),
      self.attribute_filter.as_deref(),
      None,
    )
    .with_context(|| format!("failed to stream GeoPackage layer {}", self.layer_name));

    match result {
      Ok(state) => Box::pin(RecordBatchStreamAdapter::new(
        self.schema.clone(),
        gpkg_batch_stream(state).map_err(to_datafusion_error),
      )),
      Err(err) => Box::pin(RecordBatchStreamAdapter::new(
        self.schema.clone(),
        stream::once(async { Err(to_datafusion_error(err)) }),
      )),
    }
  }
}

fn is_gpkg_path(path: &Path) -> bool {
  path.is_file()
    && path
      .extension()
      .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("gpkg"))
}

/// Open a GeoPackage with vector-only, immutable, and no-lock GDAL options.
fn open_gpkg_dataset(path: &Path) -> Result<Dataset> {
  Dataset::open_ex(
    path,
    DatasetOptions {
      open_flags: GdalOpenFlags::GDAL_OF_VECTOR | GdalOpenFlags::GDAL_OF_VERBOSE_ERROR,
      allowed_drivers: Some(&GPKG_ALLOWED_DRIVERS),
      open_options: Some(&GPKG_OPEN_OPTIONS),
      sibling_files: None,
    },
  )
  .with_context(|| format!("failed to open GeoPackage {}", path.display()))
}

fn open_gpkg_layer(path: &Path, layer_name: &str) -> Result<OwnedLayer> {
  open_gpkg_dataset(path)?
    .into_layer_by_name(layer_name)
    .with_context(|| format!("failed to open GeoPackage layer {layer_name}"))
}

fn effective_gpkg_scan_partition_count(
  total_rows: u64,
  row_range: RowRange,
  target_partitions: usize,
) -> usize {
  let effective_rows = row_range.effective_rows(total_rows);
  if effective_rows <= 1 {
    return 1;
  }
  target_partitions.max(1).min(effective_rows as usize)
}

fn quoted_sqlite_identifier(identifier: &str) -> String {
  format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Query the rowid at one logical feature offset for partition-boundary discovery.
fn query_layer_rowid_at_offset(
  dataset: &Dataset,
  layer_name: &str,
  offset: u64,
) -> Result<Option<i64>> {
  let query = format!(
    "SELECT CAST(rowid AS BIGINT) AS partition_rowid FROM {} ORDER BY rowid LIMIT 1 OFFSET {offset}",
    quoted_sqlite_identifier(layer_name)
  );
  let Some(mut result_set) = dataset.execute_sql(&query, None, sql::Dialect::SQLITE)? else {
    return Ok(None);
  };
  let field_index = result_set.defn().field_index("partition_rowid")?;
  let Some(feature) = result_set.features().next() else {
    return Ok(None);
  };
  feature.field_as_integer64(field_index).map_err(Into::into)
}

fn rowid_at_offset(
  dataset: &Dataset,
  layer_name: &str,
  total_rows: u64,
  offset: u64,
) -> Result<Option<i64>> {
  if offset < total_rows {
    query_layer_rowid_at_offset(dataset, layer_name, offset)
  } else {
    Ok(None)
  }
}

fn fallback_gpkg_scan_partitions(
  dataset: &Dataset,
  layer_name: &str,
  total_rows: u64,
  row_range: RowRange,
) -> Result<Vec<GpkgScanPartition>> {
  let start = row_range.start as u64;
  let end = start + row_range.effective_rows(total_rows);
  Ok(vec![GpkgScanPartition {
    lower_rowid: rowid_at_offset(dataset, layer_name, total_rows, start)?,
    upper_rowid: rowid_at_offset(dataset, layer_name, total_rows, end)?,
  }])
}

/// Resolve rowid boundaries that divide a requested row range across DataFusion partitions.
///
/// Boundary discovery issues SQLite offset queries before execution. If any boundary
/// cannot be resolved, the planner falls back to one contiguous partition.
fn plan_gpkg_scan_partitions(
  path: &Path,
  layer_name: &str,
  total_rows: u64,
  row_range: RowRange,
  target_partitions: usize,
) -> Result<Vec<GpkgScanPartition>> {
  let effective_rows = row_range.effective_rows(total_rows);
  let partition_count =
    effective_gpkg_scan_partition_count(total_rows, row_range, target_partitions);
  let dataset = open_gpkg_dataset(path)?;
  if partition_count <= 1 {
    return fallback_gpkg_scan_partitions(&dataset, layer_name, total_rows, row_range);
  }

  let mut partitions = Vec::with_capacity(partition_count);
  let start = row_range.start as u64;
  let end = start + effective_rows;
  let mut lower_rowid = rowid_at_offset(&dataset, layer_name, total_rows, start)?;
  for index in 1..partition_count {
    let offset = start + index as u64 * effective_rows / partition_count as u64;
    let Some(upper_rowid) = query_layer_rowid_at_offset(&dataset, layer_name, offset)? else {
      return fallback_gpkg_scan_partitions(&dataset, layer_name, total_rows, row_range);
    };
    partitions.push(GpkgScanPartition {
      lower_rowid,
      upper_rowid: Some(upper_rowid),
    });
    lower_rowid = Some(upper_rowid);
  }

  partitions.push(GpkgScanPartition {
    lower_rowid,
    upper_rowid: rowid_at_offset(&dataset, layer_name, total_rows, end)?,
  });
  Ok(partitions)
}

/// Collect user-facing metadata for every vector layer in a dataset.
fn collect_layer_summaries(dataset: &Dataset) -> Result<Vec<GpkgLayerSummary>> {
  let layer_summaries = dataset
    .layers()
    .map(|mut layer| {
      let feature_count = layer.try_feature_count();
      let declared_geometry_type = layer
        .defn()
        .geom_fields()
        .next()
        .map(|field| geometry_type_hint(field.field_type()));
      let geometry_type = match declared_geometry_type {
        Some(declared_type) if declared_type == "Geometry" => {
          sample_geometry_type_hint(&mut layer).unwrap_or(declared_type)
        }
        Some(declared_type) => declared_type,
        None => "None".to_string(),
      };

      GpkgLayerSummary {
        name: layer.name(),
        geometry_type,
        feature_count,
      }
    })
    .collect::<Vec<_>>();
  if layer_summaries.is_empty() {
    bail!("GeoPackage contains no vector layers");
  }
  Ok(layer_summaries)
}

/// Resolve an explicit layer or require one when multiple layers exist.
fn select_layer_name(
  options: &InputOpenOptions,
  layer_summaries: &[GpkgLayerSummary],
) -> Result<String> {
  let available = format_layer_summaries(layer_summaries);
  if let Some(requested) = &options.layer {
    if layer_summaries.iter().any(|layer| &layer.name == requested) {
      return Ok(requested.clone());
    }
    bail!(
      "GeoPackage layer {:?} was not found in {}\nAvailable layers:\n{}",
      requested,
      options.location,
      available
    );
  }

  if layer_summaries.len() == 1 {
    return Ok(layer_summaries[0].name.clone());
  }

  bail!(
    "GeoPackage {} contains multiple layers; pass --layer <NAME>\nAvailable layers:\n{}",
    options.location,
    available
  )
}

fn format_layer_summaries(layer_summaries: &[GpkgLayerSummary]) -> String {
  layer_summaries
    .iter()
    .map(|layer| {
      let mut hints = Vec::new();
      hints.push(format!("geometry: {}", layer.geometry_type));
      if let Some(feature_count) = layer.feature_count {
        hints.push(format!("features: {}", format_feature_count(feature_count)));
      }
      format!("- {} ({})", layer.name, hints.join(", "))
    })
    .collect::<Vec<_>>()
    .join("\n")
}

fn sample_geometry_type_hint(layer: &mut impl LayerAccess) -> Option<String> {
  match sample_geometry_type(layer) {
    Some(SampledGeometryType::Concrete {
      geometry_kind,
      has_z,
      has_m,
    }) => Some(geometry_kind_hint(geometry_kind, has_z, has_m)),
    Some(SampledGeometryType::Mixed) => Some("Mixed".to_string()),
    None => None,
  }
}

/// Inspect a bounded feature sample when GDAL reports a generic geometry type.
fn sample_geometry_type(layer: &mut impl LayerAccess) -> Option<SampledGeometryType> {
  let mut sampled_geometry: Option<(GeometryKind, bool, bool)> = None;

  for feature in layer.features().take(MAX_GEOMETRY_TYPE_SAMPLE_FEATURES) {
    let Ok(geometry) = feature.geometry_by_index(0) else {
      continue;
    };
    if geometry.is_empty() {
      continue;
    }

    let (geometry_kind, has_z, has_m) = map_geometry_type(geometry.geometry_type());
    let geometry_kind = geometry_kind?;
    match sampled_geometry {
      Some((existing_kind, existing_has_z, existing_has_m))
        if existing_kind != geometry_kind || existing_has_z != has_z || existing_has_m != has_m =>
      {
        return Some(SampledGeometryType::Mixed);
      }
      Some(_) => {}
      None => sampled_geometry = Some((geometry_kind, has_z, has_m)),
    }
  }

  sampled_geometry.map(
    |(geometry_kind, has_z, has_m)| SampledGeometryType::Concrete {
      geometry_kind,
      has_z,
      has_m,
    },
  )
}

fn format_feature_count(feature_count: u64) -> String {
  let digits = feature_count.to_string();
  let mut reversed = String::with_capacity(digits.len() + digits.len() / 3);
  for (index, ch) in digits.chars().rev().enumerate() {
    if index > 0 && index % 3 == 0 {
      reversed.push(',');
    }
    reversed.push(ch);
  }
  reversed.chars().rev().collect()
}

/// Normalize geometry type, dimensions, extent, and CRS from a GDAL layer.
fn build_geometry_metadata(
  layer: &mut impl LayerAccess,
  layer_name: &str,
) -> Result<SourceGeometryMetadata> {
  let (column_name, geometry_type, field_spatial_ref) = {
    let geom_field = layer
      .defn()
      .geom_fields()
      .next()
      .ok_or_else(|| anyhow!("GeoPackage layer {layer_name} has no geometry column"))?;
    (
      geom_field.name(),
      geom_field.field_type(),
      geom_field.spatial_ref().ok(),
    )
  };
  let (mut geometry_kind, mut has_z, mut has_m) = map_geometry_type(geometry_type);
  if geometry_kind.is_none() {
    if let Some(SampledGeometryType::Concrete {
      geometry_kind: sampled_kind,
      has_z: sampled_has_z,
      has_m: sampled_has_m,
    }) = sample_geometry_type(layer)
    {
      geometry_kind = Some(sampled_kind);
      has_z = sampled_has_z;
      has_m = sampled_has_m;
    }
  }
  let projjson = field_spatial_ref
    .or_else(|| layer.spatial_ref())
    .and_then(|spatial_ref| spatial_ref_to_projjson(&spatial_ref));
  let bbox = layer.try_get_extent()?.map(|extent| Extent2D {
    xmin: extent.MinX,
    ymin: extent.MinY,
    xmax: extent.MaxX,
    ymax: extent.MaxY,
  });

  Ok(SourceGeometryMetadata {
    column: column_name,
    encoding: GeometryEncoding::Wkb,
    geometry_types: geometry_kind.into_iter().collect(),
    bbox,
    projjson,
    has_z,
    has_m,
  })
}

fn spatial_ref_to_projjson(spatial_ref: &SpatialRef) -> Option<serde_json::Value> {
  spatial_ref
    .to_projjson()
    .ok()
    .and_then(|projjson| serde_json::from_str(&projjson).ok())
}

fn geometry_type_hint(geometry_type: OGRwkbGeometryType::Type) -> String {
  let (geometry_kind, has_z, has_m) = map_geometry_type(geometry_type);
  if let Some(geometry_kind) = geometry_kind {
    return geometry_kind_hint(geometry_kind, has_z, has_m);
  }

  let raw_name = geometry_type_to_name(geometry_type);
  match raw_name.as_str() {
    "" => "Unknown".to_string(),
    "Unknown (any)" | "Unknown" => "Geometry".to_string(),
    other => other.to_string(),
  }
}

fn geometry_kind_hint(geometry_kind: GeometryKind, has_z: bool, has_m: bool) -> String {
  let base = geometry_kind_label(geometry_kind).unwrap_or("Geometry");
  let suffix = match (has_z, has_m) {
    (false, false) => "",
    (true, false) => " Z",
    (false, true) => " M",
    (true, true) => " ZM",
  };
  format!("{base}{suffix}")
}

fn geometry_kind_label(geometry_kind: GeometryKind) -> Option<&'static str> {
  match geometry_kind {
    GeometryKind::Point => Some("Point"),
    GeometryKind::LineString => Some("LineString"),
    GeometryKind::MultiPoint => Some("MultiPoint"),
    GeometryKind::MultiLineString => Some("MultiLineString"),
    GeometryKind::Polygon => Some("Polygon"),
    GeometryKind::MultiPolygon => Some("MultiPolygon"),
    GeometryKind::GeometryCollection => Some("GeometryCollection"),
    GeometryKind::Unknown => None,
  }
}

#[allow(non_upper_case_globals)]
fn map_geometry_type(
  geometry_type: OGRwkbGeometryType::Type,
) -> (Option<GeometryKind>, bool, bool) {
  use OGRwkbGeometryType::*;

  match geometry_type {
    wkbPoint => (Some(GeometryKind::Point), false, false),
    wkbLineString => (Some(GeometryKind::LineString), false, false),
    wkbPolygon => (Some(GeometryKind::Polygon), false, false),
    wkbMultiPoint => (Some(GeometryKind::MultiPoint), false, false),
    wkbMultiLineString => (Some(GeometryKind::MultiLineString), false, false),
    wkbMultiPolygon => (Some(GeometryKind::MultiPolygon), false, false),
    wkbGeometryCollection => (Some(GeometryKind::GeometryCollection), false, false),
    wkbPoint25D => (Some(GeometryKind::Point), true, false),
    wkbLineString25D => (Some(GeometryKind::LineString), true, false),
    wkbPolygon25D => (Some(GeometryKind::Polygon), true, false),
    wkbMultiPoint25D => (Some(GeometryKind::MultiPoint), true, false),
    wkbMultiLineString25D => (Some(GeometryKind::MultiLineString), true, false),
    wkbMultiPolygon25D => (Some(GeometryKind::MultiPolygon), true, false),
    wkbGeometryCollection25D => (Some(GeometryKind::GeometryCollection), true, false),
    wkbPointM => (Some(GeometryKind::Point), false, true),
    wkbLineStringM => (Some(GeometryKind::LineString), false, true),
    wkbPolygonM => (Some(GeometryKind::Polygon), false, true),
    wkbMultiPointM => (Some(GeometryKind::MultiPoint), false, true),
    wkbMultiLineStringM => (Some(GeometryKind::MultiLineString), false, true),
    wkbMultiPolygonM => (Some(GeometryKind::MultiPolygon), false, true),
    wkbGeometryCollectionM => (Some(GeometryKind::GeometryCollection), false, true),
    wkbPointZM => (Some(GeometryKind::Point), true, true),
    wkbLineStringZM => (Some(GeometryKind::LineString), true, true),
    wkbPolygonZM => (Some(GeometryKind::Polygon), true, true),
    wkbMultiPointZM => (Some(GeometryKind::MultiPoint), true, true),
    wkbMultiLineStringZM => (Some(GeometryKind::MultiLineString), true, true),
    wkbMultiPolygonZM => (Some(GeometryKind::MultiPolygon), true, true),
    wkbGeometryCollectionZM => (Some(GeometryKind::GeometryCollection), true, true),
    _ => (None, false, false),
  }
}

/// Load the normalized Arrow schema without consuming feature batches.
fn load_schema(path: &Path, layer_name: &str, geometry_column: &str) -> Result<SchemaRef> {
  let (_layer, reader) = open_arrow_reader(path, layer_name, None)?;
  Ok(normalize_schema(reader.schema(), geometry_column))
}

/// Open a GDAL Arrow stream and normalize its geometry field into the expected schema.
fn open_arrow_reader(
  path: &Path,
  layer_name: &str,
  attribute_filter: Option<&str>,
) -> Result<(OwnedLayer, ArrowArrayStreamReader)> {
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
  Ok((layer, reader))
}

/// Retain the GDAL layer owner alongside its Arrow reader for the stream lifetime.
fn open_gpkg_batch_state(
  input_path: &Path,
  layer_name: &str,
  schema: SchemaRef,
  attribute_filter: Option<&str>,
  limit: Option<usize>,
) -> Result<GpkgBatchState> {
  let (layer, reader) = open_arrow_reader(input_path, layer_name, attribute_filter)?;
  Ok(GpkgBatchState {
    _layer: layer,
    reader,
    schema,
    remaining: limit,
  })
}

/// Convert a stateful GDAL Arrow reader into a fallible asynchronous batch stream.
fn gpkg_batch_stream(
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

    match state.reader.next() {
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

fn to_datafusion_error(err: anyhow::Error) -> DataFusionError {
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
  use super::format_feature_count;

  #[test]
  fn feature_count_formatting_uses_commas() {
    assert_eq!(format_feature_count(0), "0");
    assert_eq!(format_feature_count(12), "12");
    assert_eq!(format_feature_count(1_234), "1,234");
    assert_eq!(format_feature_count(26_348_056), "26,348,056");
  }
}
