//! Validates one optimized Parquet file or recursive partitioned dataset.

mod dataset_validator;
mod file_validator;
mod geometry_validator;
mod metadata_validator;
mod report;
mod xz_validator;
mod z_validator;

pub use dataset_validator::DatasetValidator;
pub use report::{
  ValidationFailure, ValidationFinding, ValidationLocation, ValidationReport, ValidationRule,
  ValidationSeverity,
};

/// Validate one Parquet file or recursive partitioned directory as one SOP dataset.
pub fn validate(path: impl AsRef<std::path::Path>) -> anyhow::Result<ValidationReport> {
  DatasetValidator::validate(path)
}
