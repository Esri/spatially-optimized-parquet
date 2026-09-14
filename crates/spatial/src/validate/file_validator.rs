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

use std::fs::File;

use arrow_array::{Array, Float64Array, RecordBatch, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ParquetRecordBatchReaderBuilder};

use crate::input::parquet::{ParquetDatasetFile, PartitionFamily};
use crate::optimized::GeodisplayMetadata;

use super::ValidationError;
use super::metadata_validator::{ValidatedDatasetFile, ValidatedMetadata};
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};
use super::xz_validator::XzValidator;
use super::z_validator::ZValidator;

pub(crate) struct FileValidator {
  pub(crate) file: ParquetDatasetFile,
  pub(crate) metadata: ArrowReaderMetadata,
}

impl FileValidator {
  pub(crate) fn load_all(
    files: &[ParquetDatasetFile],
    report: &mut ValidationReport,
  ) -> Vec<FileValidator> {
    files
      .iter()
      .filter_map(|file| match file.load_metadata() {
        Ok(metadata) => Some(FileValidator {
          file: file.clone(),
          metadata,
        }),
        Err(error) => {
          report.push(
            ValidationRule::RowGroup,
            ValidationSeverity::Error,
            ValidationLocation::file(file.relative_path.clone()),
            error.to_string(),
          );
          None
        }
      })
      .collect()
  }

  pub(crate) fn validate_dataset_structure(
    files: &[FileValidator],
    validated_files: &[ValidatedDatasetFile<'_>],
    report: &mut ValidationReport,
  ) {
    Self::validate_dataset_schema_consistency(files, report);
    for validated_file in validated_files {
      let file = validated_file.file;
      let metadata = &validated_file.metadata;
      Self::validate_geometry_schema(file, metadata, report);
      match &metadata.geodisplay {
        GeodisplayMetadata::Z { index } => ZValidator::validate_schema(index, file, report),
        GeodisplayMetadata::Xz { index } => XzValidator::validate_schema(index, file, report),
      }
      Self::validate_partition_family(file, metadata, report);
      Self::validate_clustering_page_indexes(file, metadata, report);
    }
  }

  pub(crate) fn validate_file_data(
    validated_files: &[ValidatedDatasetFile<'_>],
    report: &mut ValidationReport,
  ) -> Vec<super::dataset_validator::ClusteringRange> {
    let mut ranges = Vec::new();
    for validated_file in validated_files {
      let file = validated_file.file;
      let metadata = &validated_file.metadata;
      let range = match &metadata.geodisplay {
        GeodisplayMetadata::Z { index } => ZValidator::validate_file(index, file, metadata, report),
        GeodisplayMetadata::Xz { index } => {
          XzValidator::validate_file(index, file, metadata, report)
        }
      };
      if let Some(range) = range {
        ranges.push(range);
      }
    }
    ranges
  }

  fn validate_dataset_schema_consistency(files: &[FileValidator], report: &mut ValidationReport) {
    let Some(baseline) = files.first() else {
      return;
    };
    for file in files.iter().skip(1) {
      if file.metadata.schema().as_ref() != baseline.metadata.schema().as_ref() {
        report.push(
          ValidationRule::DatasetConsistency,
          ValidationSeverity::Error,
          ValidationLocation::file(file.file.relative_path.clone()),
          format!(
            "Arrow schema differs from {}",
            baseline.file.relative_path.display()
          ),
        );
      }
    }
  }

  fn validate_geometry_schema(
    file: &FileValidator,
    contract: &ValidatedMetadata,
    report: &mut ValidationReport,
  ) {
    for (column_name, geo_column) in &contract.geo.columns {
      let location = ValidationLocation::file(file.file.relative_path.clone())
        .with_column(column_name.to_string());
      match Self::field_at_path(file.metadata.schema().as_ref(), column_name) {
        Some(field)
          if matches!(
            field.data_type(),
            DataType::Binary | DataType::LargeBinary | DataType::BinaryView
          ) => {}
        Some(field) => report.push(
          ValidationRule::Schema,
          ValidationSeverity::Error,
          location,
          format!(
            "GeoParquet WKB column must use an Arrow binary type, found {}",
            field.data_type()
          ),
        ),
        None => report.push(
          ValidationRule::Schema,
          ValidationSeverity::Error,
          location,
          if column_name == contract.geometry_column() {
            "GeoParquet primary geometry column is missing"
          } else {
            "declared GeoParquet geometry column is missing"
          },
        ),
      }

      if let Some(covering) = &geo_column.covering {
        let covering_paths = [
          covering.bbox.xmin.as_slice(),
          covering.bbox.ymin.as_slice(),
          covering.bbox.xmax.as_slice(),
          covering.bbox.ymax.as_slice(),
        ];
        if !valid_covering_paths(covering_paths) {
          report.push(
          ValidationRule::Schema,
          ValidationSeverity::Error,
          ValidationLocation::file(file.file.relative_path.clone())
            .with_column(format!("{column_name}.covering.bbox")),
          "GeoParquet bbox covering must use one root struct and exact xmin/ymin/xmax/ymax child paths",
        );
          continue;
        }
        for path in covering_paths {
          let path = path.join(".");
          match Self::field_at_path(file.metadata.schema().as_ref(), &path) {
            Some(field) if field.data_type() == &DataType::Float64 => {}
            Some(field) => report.push(
              ValidationRule::Schema,
              ValidationSeverity::Error,
              ValidationLocation::file(file.file.relative_path.clone()).with_column(path),
              format!(
                "GeoParquet covering field must be Float64, found {}",
                field.data_type()
              ),
            ),
            None => report.push(
              ValidationRule::Schema,
              ValidationSeverity::Error,
              ValidationLocation::file(file.file.relative_path.clone()).with_column(path),
              "GeoParquet covering field is missing",
            ),
          }
        }

        fn valid_covering_paths(paths: [&[String]; 4]) -> bool {
          let expected_fields = ["xmin", "ymin", "xmax", "ymax"];
          let mut root = None;
          paths
            .into_iter()
            .zip(expected_fields)
            .all(|(path, expected_field)| {
              if path.len() != 2 || path[1] != expected_field {
                return false;
              }
              match root {
                Some(existing) => existing == path[0],
                None => {
                  root = Some(path[0].as_str());
                  true
                }
              }
            })
        }
      }
    }
  }

  fn validate_partition_family(
    file: &FileValidator,
    contract: &ValidatedMetadata,
    report: &mut ValidationReport,
  ) {
    let Some(partition) = file.file.partition else {
      return;
    };
    let expected = match &contract.geodisplay {
      GeodisplayMetadata::Z { .. } => PartitionFamily::Z,
      GeodisplayMetadata::Xz { .. } => PartitionFamily::Xz,
    };
    if partition.family != expected {
      report.push(
        ValidationRule::Partition,
        ValidationSeverity::Error,
        ValidationLocation::file(file.file.relative_path.clone()),
        format!(
          "{} partition directory does not match the dataset clustering family",
          partition.family.directory_prefix()
        ),
      );
    }
  }

  fn validate_clustering_page_indexes(
    file: &FileValidator,
    contract: &ValidatedMetadata,
    report: &mut ValidationReport,
  ) {
    let column_path = match &contract.geodisplay {
      GeodisplayMetadata::Z { index } => index.code.dotted(),
      GeodisplayMetadata::Xz { index } => index.code.dotted(),
    };
    let Some(column_index) = file
      .metadata
      .parquet_schema()
      .columns()
      .iter()
      .position(|column| column.path().string() == column_path)
    else {
      return;
    };

    let missing_column_index = file
      .metadata
      .metadata()
      .row_groups()
      .iter()
      .enumerate()
      .find(|(_, row_group)| {
        row_group
          .column(column_index)
          .column_index_offset()
          .is_none()
      });
    if let Some((row_group, _)) = missing_column_index {
      report.push(
        ValidationRule::PageIndex,
        ValidationSeverity::Warning,
        ValidationLocation::file(file.file.relative_path.clone())
          .with_row_group(row_group)
          .with_column(column_path.clone()),
        "missing ColumnIndex",
      );
    }

    let missing_offset_index = file
      .metadata
      .metadata()
      .row_groups()
      .iter()
      .enumerate()
      .find(|(_, row_group)| {
        row_group
          .column(column_index)
          .offset_index_offset()
          .is_none()
      });
    if let Some((row_group, _)) = missing_offset_index {
      report.push(
        ValidationRule::PageIndex,
        ValidationSeverity::Warning,
        ValidationLocation::file(file.file.relative_path.clone())
          .with_row_group(row_group)
          .with_column(column_path),
        "missing OffsetIndex",
      );
    }
  }
}

impl FileValidator {
  pub(crate) fn field_at_path<'a>(schema: &'a Schema, path: &str) -> Option<&'a Field> {
    let mut segments = path.split('.');
    let first = segments.next()?;
    let mut field = schema.field_with_name(first).ok()?;
    for segment in segments {
      let DataType::Struct(fields) = field.data_type() else {
        return None;
      };
      field = fields
        .iter()
        .find(|candidate| candidate.name() == segment)?;
    }
    Some(field)
  }

  pub(crate) fn array_at_path<'a>(
    batch: &'a RecordBatch,
    path: &str,
  ) -> Result<&'a dyn Array, ValidationError> {
    let mut segments = path.split('.');
    let first = segments
      .next()
      .filter(|segment| !segment.is_empty())
      .ok_or_else(|| ValidationError::ArrowColumn("empty Arrow column path".to_string()))?;
    let mut array = batch
      .column_by_name(first)
      .ok_or_else(|| ValidationError::ArrowColumn(format!("missing Arrow column '{first}'")))?
      .as_ref();
    for segment in segments {
      let struct_array = array
        .as_any()
        .downcast_ref::<StructArray>()
        .ok_or_else(|| {
          ValidationError::ArrowColumn(format!("Arrow column before '{segment}' is not a struct"))
        })?;
      array = struct_array
        .column_by_name(segment)
        .ok_or_else(|| {
          ValidationError::ArrowColumn(format!("missing Arrow struct field '{segment}'"))
        })?
        .as_ref();
    }
    Ok(array)
  }

  pub(crate) fn float64_array_at_path<'a>(
    batch: &'a RecordBatch,
    path: &str,
  ) -> Result<&'a Float64Array, ValidationError> {
    Self::array_at_path(batch, path)?
      .as_any()
      .downcast_ref::<Float64Array>()
      .ok_or_else(|| ValidationError::ArrowColumn(format!("Arrow column '{path}' is not Float64")))
  }

  pub(crate) fn uint64_array_at_path<'a>(
    batch: &'a RecordBatch,
    path: &str,
  ) -> Result<&'a UInt64Array, ValidationError> {
    Self::array_at_path(batch, path)?
      .as_any()
      .downcast_ref::<UInt64Array>()
      .ok_or_else(|| ValidationError::ArrowColumn(format!("Arrow column '{path}' is not UInt64")))
  }

  pub(crate) fn read_row_groups(
    &self,
    columns: &[String],
    mut visit_batch: impl FnMut(usize, u64, &RecordBatch) -> Result<(), ValidationError>,
  ) -> Result<(), ValidationError> {
    let projection = ProjectionMask::columns(
      self.metadata.parquet_schema(),
      columns.iter().map(String::as_str),
    );
    for row_group in 0..self.metadata.metadata().num_row_groups() {
      let input = File::open(&self.file.path).map_err(|source| ValidationError::Io {
        operation: "open parquet file",
        path: self.file.path.clone(),
        source,
      })?;
      let reader = ParquetRecordBatchReaderBuilder::new_with_metadata(input, self.metadata.clone())
        .with_row_groups(vec![row_group])
        .with_projection(projection.clone())
        .with_batch_size(1024)
        .build()
        .map_err(|source| ValidationError::Parquet {
          path: self.file.path.clone(),
          source,
        })?;
      let mut row_offset = 0_u64;
      for batch in reader {
        let batch = batch.map_err(|source| ValidationError::Arrow {
          operation: "read parquet rows",
          path: self.file.path.clone(),
          source,
        })?;
        visit_batch(row_group, row_offset, &batch)?;
        row_offset += batch.num_rows() as u64;
      }
    }
    Ok(())
  }
}
