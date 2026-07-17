use std::fmt;
use std::path::{Path, PathBuf};

/// Identifies whether a validation finding invalidates the dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationSeverity {
  /// Rejects the dataset as non-conforming.
  Error,
  /// Reports an interoperability or optimization concern without rejecting the dataset.
  Warning,
}

impl fmt::Display for ValidationSeverity {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Error => formatter.write_str("error"),
      Self::Warning => formatter.write_str("warning"),
    }
  }
}

/// Identifies one stable SOP validation rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationRule {
  /// Requires reserved metadata entries.
  MetadataMissing,
  /// Rejects duplicate reserved metadata entries.
  MetadataDuplicate,
  /// Rejects malformed reserved metadata values.
  MetadataMalformed,
  /// Enforces supported metadata versions.
  MetadataVersion,
  /// Enforces metadata field contracts.
  MetadataContract,
  /// Recommends optional writer identification.
  WriterMetadata,
  /// Enforces supported and matching coordinate reference systems.
  Crs,
  /// Enforces finite ordered and matching extents.
  Extent,
  /// Enforces Arrow schema paths and data types.
  Schema,
  /// Enforces metadata and schema consistency across files.
  DatasetConsistency,
  /// Reports Parquet footer, row-group, or decoding failures.
  RowGroup,
  /// Recommends clustering page indexes.
  PageIndex,
  /// Rejects malformed sampled geometry.
  Geometry,
  /// Enforces geometry family declarations.
  GeometryType,
  /// Enforces supported geometry dimensions.
  GeometryDimension,
  /// Enforces finite geometry coordinates.
  GeometryCoordinate,
  /// Enforces polygon ring closure.
  RingClosure,
  /// Reports unexpected full-resolution WKB winding.
  WkbWinding,
  /// Enforces point clustering schema.
  ZSchema,
  /// Enforces non-decreasing Z-code order.
  ZOrder,
  /// Enforces stored point coordinate values.
  ZCoordinate,
  /// Enforces recomputed Z-code values.
  ZCode,
  /// Enforces XZ group and level schema.
  XzSchema,
  /// Enforces XZ metadata constants and values.
  XzMetadata,
  /// Enforces non-decreasing XZ-code order.
  XzOrder,
  /// Enforces recomputed XZ-code values.
  XzCode,
  /// Enforces Esri PBF wire and payload structure.
  Pbf,
  /// Enforces retained coordinates for degenerated Esri PBF geometry.
  PbfDegenerate,
  /// Reports unexpected Esri PBF winding.
  PbfWinding,
  /// Reports unavailable non-empty bounded Esri PBF samples.
  PbfSample,
  /// Enforces clustering partition containment.
  Partition,
  /// Reports overlapping dataset clustering ranges.
  RangeOverlap,
}

impl ValidationRule {
  /// Return the stable human identifier for this rule.
  pub const fn id(self) -> &'static str {
    match self {
      Self::MetadataMissing => "SOP-META-001",
      Self::MetadataDuplicate => "SOP-META-002",
      Self::MetadataMalformed => "SOP-META-003",
      Self::MetadataVersion => "SOP-META-004",
      Self::MetadataContract => "SOP-META-005",
      Self::WriterMetadata => "SOP-META-006",
      Self::Crs => "SOP-CRS-001",
      Self::Extent => "SOP-EXTENT-001",
      Self::Schema => "SOP-SCHEMA-001",
      Self::DatasetConsistency => "SOP-DATASET-001",
      Self::RowGroup => "SOP-PARQUET-001",
      Self::PageIndex => "SOP-PARQUET-002",
      Self::Geometry => "SOP-GEOMETRY-001",
      Self::GeometryType => "SOP-GEOMETRY-002",
      Self::GeometryDimension => "SOP-GEOMETRY-003",
      Self::GeometryCoordinate => "SOP-GEOMETRY-004",
      Self::RingClosure => "SOP-GEOMETRY-005",
      Self::WkbWinding => "SOP-GEOMETRY-006",
      Self::ZSchema => "SOP-Z-001",
      Self::ZOrder => "SOP-Z-002",
      Self::ZCoordinate => "SOP-Z-003",
      Self::ZCode => "SOP-Z-004",
      Self::XzSchema => "SOP-XZ-001",
      Self::XzMetadata => "SOP-XZ-002",
      Self::XzOrder => "SOP-XZ-003",
      Self::XzCode => "SOP-XZ-004",
      Self::Pbf => "SOP-PBF-001",
      Self::PbfDegenerate => "SOP-PBF-002",
      Self::PbfWinding => "SOP-PBF-003",
      Self::PbfSample => "SOP-PBF-004",
      Self::Partition => "SOP-PARTITION-001",
      Self::RangeOverlap => "SOP-RANGE-001",
    }
  }
}

impl fmt::Display for ValidationRule {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.id())
  }
}

/// Locates one finding within a dataset.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ValidationLocation {
  file: Option<PathBuf>,
  row_group: Option<usize>,
  row: Option<u64>,
  column: Option<String>,
}

impl ValidationLocation {
  pub(crate) fn file(path: impl Into<PathBuf>) -> Self {
    Self {
      file: Some(path.into()),
      ..Self::default()
    }
  }

  pub(crate) fn with_row_group(mut self, row_group: usize) -> Self {
    self.row_group = Some(row_group);
    self
  }

  pub(crate) fn with_row(mut self, row: u64) -> Self {
    self.row = Some(row);
    self
  }

  pub(crate) fn with_column(mut self, column: impl Into<String>) -> Self {
    self.column = Some(column.into());
    self
  }

  /// Return the relative file path when the finding belongs to one file.
  pub fn file_path(&self) -> Option<&Path> {
    self.file.as_deref()
  }

  /// Return the zero-based row-group index when available.
  pub const fn row_group(&self) -> Option<usize> {
    self.row_group
  }

  /// Return the zero-based row index within the row group when available.
  pub const fn row(&self) -> Option<u64> {
    self.row
  }

  /// Return the Arrow or metadata column path when available.
  pub fn column(&self) -> Option<&str> {
    self.column.as_deref()
  }
}

impl fmt::Display for ValidationLocation {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut wrote_value = false;
    if let Some(file) = &self.file {
      write!(formatter, "{}", file.display())?;
      wrote_value = true;
    }
    if let Some(row_group) = self.row_group {
      if wrote_value {
        formatter.write_str(" ")?;
      }
      write!(formatter, "row-group={row_group}")?;
      wrote_value = true;
    }
    if let Some(row) = self.row {
      if wrote_value {
        formatter.write_str(" ")?;
      }
      write!(formatter, "row={row}")?;
      wrote_value = true;
    }
    if let Some(column) = &self.column {
      if wrote_value {
        formatter.write_str(" ")?;
      }
      formatter.write_str(column)?;
    }
    Ok(())
  }
}

/// Describes one validation error or warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationFinding {
  rule: ValidationRule,
  severity: ValidationSeverity,
  message: String,
  location: ValidationLocation,
}

impl ValidationFinding {
  pub(crate) fn new(
    rule: ValidationRule,
    severity: ValidationSeverity,
    location: ValidationLocation,
    message: impl Into<String>,
  ) -> Self {
    Self {
      rule,
      severity,
      message: message.into(),
      location,
    }
  }

  /// Return the stable rule identity.
  pub const fn rule(&self) -> ValidationRule {
    self.rule
  }

  /// Return the finding severity.
  pub const fn severity(&self) -> ValidationSeverity {
    self.severity
  }

  /// Return the contextual finding message.
  pub fn message(&self) -> &str {
    &self.message
  }

  /// Return the dataset location associated with the finding.
  pub const fn location(&self) -> &ValidationLocation {
    &self.location
  }
}

/// Represents the complete deterministic validation result for one dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
  dataset_path: PathBuf,
  findings: Vec<ValidationFinding>,
}

impl ValidationReport {
  pub(crate) fn new(dataset_path: PathBuf) -> Self {
    Self {
      dataset_path,
      findings: Vec::new(),
    }
  }

  pub(crate) fn push(
    &mut self,
    rule: ValidationRule,
    severity: ValidationSeverity,
    location: ValidationLocation,
    message: impl Into<String>,
  ) {
    self
      .findings
      .push(ValidationFinding::new(rule, severity, location, message));
  }

  pub(crate) fn sort_findings(&mut self) {
    self.findings.sort_by(|left, right| {
      (
        left.severity,
        &left.location.file,
        left.location.row_group,
        left.location.row,
        &left.location.column,
        left.rule,
        &left.message,
      )
        .cmp(&(
          right.severity,
          &right.location.file,
          right.location.row_group,
          right.location.row,
          &right.location.column,
          right.rule,
          &right.message,
        ))
    });
  }

  /// Return the validated file or directory path.
  pub fn dataset_path(&self) -> &Path {
    &self.dataset_path
  }

  /// Return findings in deterministic display order.
  pub fn findings(&self) -> &[ValidationFinding] {
    &self.findings
  }

  /// Count validation errors.
  pub fn error_count(&self) -> usize {
    self
      .findings
      .iter()
      .filter(|finding| finding.severity == ValidationSeverity::Error)
      .count()
  }

  /// Count validation warnings.
  pub fn warning_count(&self) -> usize {
    self
      .findings
      .iter()
      .filter(|finding| finding.severity == ValidationSeverity::Warning)
      .count()
  }

  /// Return whether any finding invalidates the dataset.
  pub fn has_errors(&self) -> bool {
    self.error_count() != 0
  }

  /// Return this report when valid or a typed failure when errors exist.
  pub fn ensure_valid(self) -> Result<Self, ValidationFailure> {
    if self.has_errors() {
      Err(ValidationFailure { report: self })
    } else {
      Ok(self)
    }
  }
}

impl fmt::Display for ValidationReport {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let status = if self.has_errors() {
      "invalid"
    } else {
      "valid"
    };
    writeln!(formatter, "{status}: {}", self.dataset_path.display())?;
    writeln!(
      formatter,
      "{} errors, {} warnings",
      self.error_count(),
      self.warning_count()
    )?;
    for finding in &self.findings {
      write!(formatter, "{} {}", finding.severity, finding.rule)?;
      let location = finding.location.to_string();
      if !location.is_empty() {
        write!(formatter, " {location}")?;
      }
      writeln!(formatter, ": {}", finding.message)?;
    }
    Ok(())
  }
}

/// Owns the complete report for an invalid dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationFailure {
  report: ValidationReport,
}

impl ValidationFailure {
  /// Return the invalid validation report.
  pub const fn report(&self) -> &ValidationReport {
    &self.report
  }

  /// Consume the failure and return its report.
  pub fn into_report(self) -> ValidationReport {
    self.report
  }
}

impl fmt::Display for ValidationFailure {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.report.fmt(formatter)
  }
}

impl std::error::Error for ValidationFailure {}
