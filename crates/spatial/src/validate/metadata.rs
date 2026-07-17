use std::collections::BTreeSet;

use serde::de::DeserializeOwned;

use crate::geometry::Extent2D;
use crate::geoparquet::GeoMetadata;
use crate::optimized::DEFAULT_XZ_MAX_LEVEL;
use crate::output::{
  ESRI_PBF_ENCODING, GEODISPLAY_VERSION, GeodisplayIndex, GeodisplayMetadata,
  QUANTIZED_NATIVE_ENCODING, XzClusteringIndex, ZClusteringIndex,
};

use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};
use super::structure::LoadedDatasetFile;

const GEO_VERSION: &str = "1.1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidatedCrs {
  Wgs84,
  WebMercator,
}

impl ValidatedCrs {
  pub(crate) const fn epsg(self) -> u32 {
    match self {
      Self::Wgs84 => 4326,
      Self::WebMercator => 3857,
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ValidatedMetadata {
  pub(crate) geo: GeoMetadata,
  pub(crate) geodisplay: GeodisplayMetadata,
  pub(crate) crs: ValidatedCrs,
}

pub(crate) struct ValidatedDatasetFile<'file> {
  pub(crate) file: &'file LoadedDatasetFile,
  pub(crate) metadata: ValidatedMetadata,
}

impl ValidatedMetadata {
  pub(crate) fn geometry_column(&self) -> &str {
    &self.geo.primary_column
  }

  pub(crate) fn geometry_types(&self) -> &[String] {
    self
      .geo
      .columns
      .get(&self.geo.primary_column)
      .map_or(&[], |column| column.geometry_types.as_slice())
  }
}

pub(crate) fn validate_dataset_metadata<'file>(
  files: &'file [LoadedDatasetFile],
  report: &mut ValidationReport,
) -> Vec<ValidatedDatasetFile<'file>> {
  let validated_files = files
    .iter()
    .filter_map(|file| {
      validate_file_metadata(file, report).map(|metadata| ValidatedDatasetFile { file, metadata })
    })
    .collect::<Vec<_>>();

  if let Some(baseline) = validated_files.first() {
    for validated_file in validated_files.iter().skip(1) {
      if validated_file.metadata != baseline.metadata {
        report.push(
          ValidationRule::DatasetConsistency,
          ValidationSeverity::Error,
          ValidationLocation::file(validated_file.file.file.relative_path.clone()),
          format!(
            "GeoParquet or geodisplay metadata differs from {}",
            baseline.file.file.relative_path.display()
          ),
        );
      }
    }
  }

  validated_files
}

fn validate_file_metadata(
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) -> Option<ValidatedMetadata> {
  let initial_error_count = report.error_count();
  let entries = file
    .metadata
    .metadata()
    .file_metadata()
    .key_value_metadata()
    .map_or(&[][..], Vec::as_slice);
  let geo = parse_reserved_metadata::<GeoMetadata>("geo", entries, file, report);
  let geodisplay =
    parse_reserved_metadata::<GeodisplayMetadata>("geodisplay", entries, file, report);
  let (Some(geo), Some(geodisplay)) = (geo, geodisplay) else {
    return None;
  };

  validate_geo_contract(&geo, file, report);
  validate_geodisplay_contract(&geodisplay, file, report);
  validate_geometry_family(&geo, &geodisplay, file, report);

  let geo_crs = resolve_geo_crs(&geo, file, report);
  let geodisplay_crs = resolve_geodisplay_crs(&geodisplay, file, report);
  let crs = match (geo_crs, geodisplay_crs) {
    (Some(geo_crs), Some(geodisplay_crs)) if geo_crs == geodisplay_crs => Some(geo_crs),
    (Some(geo_crs), Some(geodisplay_crs)) => {
      report.push(
        ValidationRule::Crs,
        ValidationSeverity::Error,
        ValidationLocation::file(file.file.relative_path.clone()),
        format!(
          "GeoParquet EPSG:{} does not match geodisplay EPSG:{}",
          geo_crs.epsg(),
          geodisplay_crs.epsg()
        ),
      );
      None
    }
    _ => None,
  };

  validate_matching_extent(&geo, &geodisplay, file, report);

  if report.error_count() != initial_error_count {
    return None;
  }

  crs.map(|crs| ValidatedMetadata {
    geo,
    geodisplay,
    crs,
  })
}

fn parse_reserved_metadata<T: DeserializeOwned>(
  key: &str,
  entries: &[parquet::file::metadata::KeyValue],
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) -> Option<T> {
  let matching = entries
    .iter()
    .filter(|entry| entry.key == key)
    .collect::<Vec<_>>();
  let location =
    ValidationLocation::file(file.file.relative_path.clone()).with_column(key.to_string());
  if matching.is_empty() {
    report.push(
      ValidationRule::MetadataMissing,
      ValidationSeverity::Error,
      location,
      format!("missing required root metadata entry '{key}'"),
    );
    return None;
  }
  if matching.len() > 1 {
    report.push(
      ValidationRule::MetadataDuplicate,
      ValidationSeverity::Error,
      location.clone(),
      format!("root metadata contains {} '{key}' entries", matching.len()),
    );
  }
  let Some(value) = matching[0].value.as_deref() else {
    report.push(
      ValidationRule::MetadataMalformed,
      ValidationSeverity::Error,
      location,
      format!("root metadata entry '{key}' has no value"),
    );
    return None;
  };
  match serde_json::from_str(value) {
    Ok(value) => Some(value),
    Err(error) => {
      report.push(
        ValidationRule::MetadataMalformed,
        ValidationSeverity::Error,
        location,
        format!("invalid '{key}' JSON: {error}"),
      );
      None
    }
  }
}

fn validate_geo_contract(
  geo: &GeoMetadata,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) {
  let location = ValidationLocation::file(file.file.relative_path.clone()).with_column("geo");
  if geo.version != GEO_VERSION {
    report.push(
      ValidationRule::MetadataVersion,
      ValidationSeverity::Error,
      location.clone(),
      format!(
        "GeoParquet version must be {GEO_VERSION}, found {}",
        geo.version
      ),
    );
  }
  if !geo.columns.contains_key(&geo.primary_column) {
    report.push(
      ValidationRule::MetadataContract,
      ValidationSeverity::Error,
      location.clone(),
      format!(
        "primary geometry column '{}' is not declared in geo.columns",
        geo.primary_column
      ),
    );
  }
  for (column_name, column) in &geo.columns {
    let column_location = ValidationLocation::file(file.file.relative_path.clone())
      .with_column(format!("geo.columns.{column_name}"));
    if column.encoding != "WKB" {
      report.push(
        ValidationRule::MetadataContract,
        ValidationSeverity::Error,
        column_location.clone(),
        format!("geometry encoding must be WKB, found {}", column.encoding),
      );
    }
    if column.geometry_types.is_empty() {
      report.push(
        ValidationRule::MetadataContract,
        ValidationSeverity::Error,
        column_location.clone(),
        "geometry_types must contain at least one geometry type",
      );
    }
    for geometry_type in &column.geometry_types {
      if !matches!(
        geometry_base_type(geometry_type),
        "Point" | "MultiPoint" | "LineString" | "MultiLineString" | "Polygon" | "MultiPolygon"
      ) {
        report.push(
          ValidationRule::MetadataContract,
          ValidationSeverity::Error,
          column_location.clone(),
          format!("unsupported GeoParquet geometry type '{geometry_type}'"),
        );
      }
    }
    validate_extent(
      Extent2D {
        xmin: column.bbox[0],
        ymin: column.bbox[1],
        xmax: column.bbox[2],
        ymax: column.bbox[3],
      },
      column_location,
      "GeoParquet bbox",
      report,
    );
  }
}

fn validate_geodisplay_contract(
  geodisplay: &GeodisplayMetadata,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) {
  match &geodisplay.index {
    GeodisplayIndex::Z(index) => validate_z_metadata(index, file, report),
    GeodisplayIndex::Xz(index) => validate_xz_metadata(index, file, report),
  }
}

fn validate_common_metadata(
  version: &str,
  writer: Option<(&str, &str)>,
  full_extent: Extent2D,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) {
  let location =
    ValidationLocation::file(file.file.relative_path.clone()).with_column("geodisplay");
  if version != GEODISPLAY_VERSION {
    report.push(
      ValidationRule::MetadataVersion,
      ValidationSeverity::Error,
      location.clone(),
      format!("geodisplay version must be {GEODISPLAY_VERSION}, found {version}"),
    );
  }
  match writer {
    None => report.push(
      ValidationRule::WriterMetadata,
      ValidationSeverity::Warning,
      location.clone(),
      "optional geodisplay writer metadata is missing",
    ),
    Some((name, version)) if name.is_empty() || version.is_empty() => report.push(
      ValidationRule::MetadataContract,
      ValidationSeverity::Error,
      location.clone().with_column("geodisplay.writer"),
      "writer name and version must not be empty",
    ),
    Some(_) => {}
  }
  validate_extent(full_extent, location, "geodisplay fullExtent", report);
}

fn validate_z_metadata(
  index: &ZClusteringIndex,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) {
  validate_common_metadata(
    &index.version,
    index
      .writer
      .as_ref()
      .map(|writer| (writer.name.as_str(), writer.version.as_str())),
    index.full_extent,
    file,
    report,
  );
  let location =
    ValidationLocation::file(file.file.relative_path.clone()).with_column("geodisplay");
  if index.index_type != "z" || index.geometry_type != "point" {
    report.push(
      ValidationRule::MetadataContract,
      ValidationSeverity::Error,
      location.clone(),
      "Z metadata requires type 'z' and geometryType 'point'",
    );
  }
  if index.coordinate_precision == 0 || index.coordinate_precision > 32 {
    report.push(
      ValidationRule::MetadataContract,
      ValidationSeverity::Error,
      location.clone(),
      "coordinatePrecision must be between 1 and 32",
    );
  }
  for (name, value) in [
    ("code", index.code.as_str()),
    ("xColumn", index.x_column.as_str()),
    ("yColumn", index.y_column.as_str()),
  ] {
    if value.is_empty() {
      report.push(
        ValidationRule::MetadataContract,
        ValidationSeverity::Error,
        location.clone().with_column(format!("geodisplay.{name}")),
        format!("{name} must not be empty"),
      );
    }
  }
  if index.has_z != index.z_column.is_some() || index.has_m != index.m_column.is_some() {
    report.push(
      ValidationRule::MetadataContract,
      ValidationSeverity::Error,
      location,
      "zColumn and mColumn presence must match hasZ and hasM",
    );
  }
}

fn validate_xz_metadata(
  index: &XzClusteringIndex,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) {
  validate_common_metadata(
    &index.version,
    index
      .writer
      .as_ref()
      .map(|writer| (writer.name.as_str(), writer.version.as_str())),
    index.full_extent,
    file,
    report,
  );
  let location =
    ValidationLocation::file(file.file.relative_path.clone()).with_column("geodisplay");
  if index.index_type != "xz" {
    report.push(
      ValidationRule::MetadataContract,
      ValidationSeverity::Error,
      location.clone(),
      "XZ metadata type must be 'xz'",
    );
  }
  if !matches!(
    index.geometry_type.as_str(),
    "multipoint" | "polyline" | "polygon"
  ) {
    report.push(
      ValidationRule::MetadataContract,
      ValidationSeverity::Error,
      location.clone(),
      format!("unsupported XZ geometryType '{}'", index.geometry_type),
    );
  }
  if index.max_level != DEFAULT_XZ_MAX_LEVEL {
    report.push(
      ValidationRule::XzMetadata,
      ValidationSeverity::Error,
      location.clone(),
      format!(
        "maxLevel must be {DEFAULT_XZ_MAX_LEVEL}, found {}",
        index.max_level
      ),
    );
  }
  if !matches!(
    index.encoding.as_str(),
    ESRI_PBF_ENCODING | QUANTIZED_NATIVE_ENCODING
  ) {
    report.push(
      ValidationRule::XzMetadata,
      ValidationSeverity::Error,
      location.clone(),
      format!(
        "encoding must be {ESRI_PBF_ENCODING} or {QUANTIZED_NATIVE_ENCODING}, found {}",
        index.encoding
      ),
    );
  }
  if index.code.is_empty() {
    report.push(
      ValidationRule::MetadataContract,
      ValidationSeverity::Error,
      location.clone(),
      "code must not be empty",
    );
  }
  if index.levels.is_empty() {
    report.push(
      ValidationRule::XzMetadata,
      ValidationSeverity::Error,
      location.clone(),
      "levels must contain at least one multiscale level",
    );
  }
  let mut levels = BTreeSet::new();
  let mut columns = BTreeSet::new();
  for level in &index.levels {
    let level_location = location
      .clone()
      .with_column(format!("geodisplay.{}", level.column));
    if u32::from(level.level) > index.max_level || !levels.insert(level.level) {
      report.push(
        ValidationRule::XzMetadata,
        ValidationSeverity::Error,
        level_location.clone(),
        format!(
          "level {} must be unique and between 0 and {}",
          level.level, index.max_level
        ),
      );
    }
    if level.column.is_empty() || !columns.insert(level.column.as_str()) {
      report.push(
        ValidationRule::XzMetadata,
        ValidationSeverity::Error,
        level_location.clone(),
        "multiscale column names must be non-empty and unique",
      );
    }
    if !level.resolution.is_finite()
      || level.resolution <= 0.0
      || !level.scale.is_finite()
      || level.scale <= 0.0
    {
      report.push(
        ValidationRule::XzMetadata,
        ValidationSeverity::Error,
        level_location.clone(),
        "resolution and scale must be finite positive values",
      );
    }
    if level
      .transform
      .scale
      .iter()
      .chain(level.transform.translate.iter())
      .any(|value| !value.is_finite())
      || level.transform.scale[0] <= 0.0
      || level.transform.scale[1] <= 0.0
      || (index.has_z && level.transform.scale[2] <= 0.0)
      || (index.has_m && level.transform.scale[3] <= 0.0)
    {
      report.push(
        ValidationRule::XzMetadata,
        ValidationSeverity::Error,
        level_location,
        "transform values must be finite and scales for every present dimension must be positive",
      );
    }
  }
}

fn validate_geometry_family(
  geo: &GeoMetadata,
  geodisplay: &GeodisplayMetadata,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) {
  let Some(column) = geo.columns.get(&geo.primary_column) else {
    return;
  };
  let matches_family = column.geometry_types.iter().all(|geometry_type| {
    let base = geometry_base_type(geometry_type);
    match &geodisplay.index {
      GeodisplayIndex::Z(_) => base == "Point",
      GeodisplayIndex::Xz(index) => match index.geometry_type.as_str() {
        "multipoint" => base == "MultiPoint",
        "polyline" => matches!(base, "LineString" | "MultiLineString"),
        "polygon" => matches!(base, "Polygon" | "MultiPolygon"),
        _ => false,
      },
    }
  });
  if !matches_family {
    report.push(
      ValidationRule::GeometryType,
      ValidationSeverity::Error,
      ValidationLocation::file(file.file.relative_path.clone()).with_column("geo"),
      "GeoParquet geometry_types do not match the geodisplay geometryType",
    );
  }
}

fn resolve_geo_crs(
  geo: &GeoMetadata,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) -> Option<ValidatedCrs> {
  let Some(column) = geo.columns.get(&geo.primary_column) else {
    return None;
  };
  let location = ValidationLocation::file(file.file.relative_path.clone()).with_column("geo.crs");
  let crs = match column.crs.get("id") {
    Some(id) => {
      let authority = id.get("authority").and_then(serde_json::Value::as_str);
      let epsg = id.get("code").and_then(json_u32);
      match (authority, epsg) {
        (Some(authority), Some(epsg)) if authority.eq_ignore_ascii_case("EPSG") => {
          supported_crs(epsg)
        }
        _ => None,
      }
    }
    None => column
      .crs
      .get("name")
      .and_then(serde_json::Value::as_str)
      .and_then(resolve_wkt_name),
  };
  if crs.is_none() {
    report.push(
      ValidationRule::Crs,
      ValidationSeverity::Error,
      location,
      "GeoParquet CRS must resolve to EPSG:4326 or EPSG:3857",
    );
  }
  crs
}

fn resolve_geodisplay_crs(
  geodisplay: &GeodisplayMetadata,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) -> Option<ValidatedCrs> {
  let (wkid, wkt) = match &geodisplay.index {
    GeodisplayIndex::Z(index) => (index.wkid, index.wkt.as_deref()),
    GeodisplayIndex::Xz(index) => (index.wkid, index.wkt.as_deref()),
  };
  let location =
    ValidationLocation::file(file.file.relative_path.clone()).with_column("geodisplay");
  if wkid.is_some() && wkt.is_some() {
    report.push(
      ValidationRule::Crs,
      ValidationSeverity::Error,
      location,
      "geodisplay must define either wkid or wkt, not both",
    );
    return None;
  }
  let wkid_crs = wkid.and_then(supported_crs);
  if wkid.is_some() && wkid_crs.is_none() {
    report.push(
      ValidationRule::Crs,
      ValidationSeverity::Error,
      location.clone(),
      format!(
        "geodisplay WKID {} is unsupported",
        wkid.expect("checked as present")
      ),
    );
  }
  let wkt_crs = wkt.and_then(resolve_wkt_name);
  if wkt.is_some() && wkt_crs.is_none() {
    report.push(
      ValidationRule::Crs,
      ValidationSeverity::Error,
      location.clone().with_column("geodisplay.wkt"),
      "geodisplay WKT must resolve to EPSG:4326 or EPSG:3857",
    );
  }
  if wkid.is_none() && wkt.is_none() {
    report.push(
      ValidationRule::Crs,
      ValidationSeverity::Error,
      location,
      "geodisplay must define wkid or wkt",
    );
    return None;
  }
  wkid_crs.or(wkt_crs)
}

fn validate_matching_extent(
  geo: &GeoMetadata,
  geodisplay: &GeodisplayMetadata,
  file: &LoadedDatasetFile,
  report: &mut ValidationReport,
) {
  let Some(column) = geo.columns.get(&geo.primary_column) else {
    return;
  };
  let display_extent = match &geodisplay.index {
    GeodisplayIndex::Z(index) => index.full_extent,
    GeodisplayIndex::Xz(index) => index.full_extent,
  };
  let geo_extent = [
    column.bbox[0],
    column.bbox[1],
    column.bbox[2],
    column.bbox[3],
  ];
  let display_values = [
    display_extent.xmin,
    display_extent.ymin,
    display_extent.xmax,
    display_extent.ymax,
  ];
  if geo_extent
    .iter()
    .zip(display_values)
    .any(|(left, right)| !float_matches(*left, right))
  {
    report.push(
      ValidationRule::Extent,
      ValidationSeverity::Error,
      ValidationLocation::file(file.file.relative_path.clone())
        .with_column("geodisplay.fullExtent"),
      "GeoParquet bbox does not match geodisplay fullExtent",
    );
  }
}

fn validate_extent(
  extent: Extent2D,
  location: ValidationLocation,
  label: &str,
  report: &mut ValidationReport,
) {
  let values = [extent.xmin, extent.ymin, extent.xmax, extent.ymax];
  if values.iter().any(|value| !value.is_finite())
    || extent.xmin > extent.xmax
    || extent.ymin > extent.ymax
  {
    report.push(
      ValidationRule::Extent,
      ValidationSeverity::Error,
      location,
      format!("{label} must contain finite ordered bounds"),
    );
  }
}

fn supported_crs(wkid: u32) -> Option<ValidatedCrs> {
  match wkid {
    4326 => Some(ValidatedCrs::Wgs84),
    3857 => Some(ValidatedCrs::WebMercator),
    _ => None,
  }
}

fn resolve_wkt_name(value: &str) -> Option<ValidatedCrs> {
  let normalized = value.to_ascii_uppercase();
  if normalized.contains("3857")
    || normalized.contains("PSEUDO-MERCATOR")
    || normalized.contains("WEB MERCATOR")
  {
    Some(ValidatedCrs::WebMercator)
  } else if normalized.contains("4326")
    || normalized.contains("WGS 84")
    || normalized.contains("WGS_1984")
  {
    Some(ValidatedCrs::Wgs84)
  } else {
    None
  }
}

fn json_u32(value: &serde_json::Value) -> Option<u32> {
  value
    .as_u64()
    .and_then(|value| u32::try_from(value).ok())
    .or_else(|| value.as_str()?.parse().ok())
}

pub(crate) fn geometry_base_type(value: &str) -> &str {
  value.split_whitespace().next().unwrap_or(value)
}

pub(crate) fn float_matches(left: f64, right: f64) -> bool {
  let scale = left.abs().max(right.abs()).max(1.0);
  (left - right).abs() <= scale * 1.0e-9
}
