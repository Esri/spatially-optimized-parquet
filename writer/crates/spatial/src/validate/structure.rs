use std::fs::File;

use anyhow::{Context, Result};
use arrow_array::{Array, RecordBatch, StructArray};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ParquetRecordBatchReaderBuilder};

use crate::output::GeodisplayIndex;
use crate::parquet_dataset::{ParquetDatasetFile, PartitionFamily, load_parquet_metadata};

use super::metadata::ValidatedMetadata;
use super::multifile::FileCodeRange;
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};

pub(crate) struct LoadedDatasetFile {
  pub(crate) file: ParquetDatasetFile,
  pub(crate) metadata: ArrowReaderMetadata,
}

pub(crate) fn load_dataset_files(
  files: &[ParquetDatasetFile],
  report: &mut ValidationReport,
) -> Vec<LoadedDatasetFile> {
  files
    .iter()
    .filter_map(|file| match load_parquet_metadata(&file.path) {
      Ok(metadata) => Some(LoadedDatasetFile {
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
  files: &[LoadedDatasetFile],
  contracts: &[Option<ValidatedMetadata>],
  report: &mut ValidationReport,
) {
  validate_dataset_schema_consistency(files, report);
  for (file, contract) in files.iter().zip(contracts) {
    let Some(contract) = contract else {
      continue;
    };
    validate_geometry_schema(file, contract, report);
    match &contract.geodisplay.index {
      GeodisplayIndex::Z(index) => super::z::validate_z_schema(
        file,
        contract.geodisplay.parent_column.as_deref(),
        index,
        report,
      ),
      GeodisplayIndex::Xz(index) => super::xz::validate_xz_schema(
        file,
        contract.geodisplay.parent_column.as_deref(),
        index,
        report,
      ),
    }
    validate_partition_family(file, contract, report);
    validate_clustering_page_indexes(file, contract, report);
  }
}

pub(crate) fn validate_file_data(
  files: &[LoadedDatasetFile],
  contracts: &[Option<ValidatedMetadata>],
  report: &mut ValidationReport,
) -> Vec<FileCodeRange> {
  let mut ranges = Vec::new();
  for (file, contract) in files.iter().zip(contracts) {
    let Some(contract) = contract else {
      continue;
    };
    let range = match &contract.geodisplay.index {
      GeodisplayIndex::Z(index) => super::z::validate_z_file(
        file,
        contract,
        contract.geodisplay.parent_column.as_deref(),
        index,
        report,
      ),
      GeodisplayIndex::Xz(index) => super::xz::validate_xz_file(
        file,
        contract,
        contract.geodisplay.parent_column.as_deref(),
        index,
        report,
      ),
    };
    if let Some(range) = range {
      ranges.push(range);
    }
  }
  ranges
}

fn validate_dataset_schema_consistency(files: &[LoadedDatasetFile], report: &mut ValidationReport) {
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
  file: &LoadedDatasetFile,
  contract: &ValidatedMetadata,
  report: &mut ValidationReport,
) {
  for (column_name, geo_column) in &contract.geo.columns {
    let location = ValidationLocation::file(file.file.relative_path.clone())
      .with_column(column_name.to_string());
    match field_at_path(file.metadata.schema().as_ref(), column_name) {
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
        match field_at_path(file.metadata.schema().as_ref(), &path) {
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
  file: &LoadedDatasetFile,
  contract: &ValidatedMetadata,
  report: &mut ValidationReport,
) {
  let Some(partition) = file.file.partition else {
    return;
  };
  let expected = match &contract.geodisplay.index {
    GeodisplayIndex::Z(_) => PartitionFamily::Z,
    GeodisplayIndex::Xz(_) => PartitionFamily::Xz,
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
  file: &LoadedDatasetFile,
  contract: &ValidatedMetadata,
  report: &mut ValidationReport,
) {
  let column_path = match &contract.geodisplay.index {
    GeodisplayIndex::Z(index) => {
      display_column_path(contract.geodisplay.parent_column.as_deref(), &index.code)
    }
    GeodisplayIndex::Xz(index) => {
      display_column_path(contract.geodisplay.parent_column.as_deref(), &index.code)
    }
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

pub(crate) fn display_column_path(parent: Option<&str>, child: &str) -> String {
  parent.map_or_else(|| child.to_string(), |parent| format!("{parent}.{child}"))
}

#[cfg(test)]
mod tests {
  use super::display_column_path;

  #[test]
  fn resolves_display_columns_with_any_optional_parent() {
    assert_eq!(display_column_path(None, "zCode"), "zCode");
    assert_eq!(
      display_column_path(Some("display"), "zCode"),
      "display.zCode"
    );
    assert_eq!(
      display_column_path(Some("customOptimization"), "xzCode"),
      "customOptimization.xzCode"
    );
  }
}

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

pub(crate) fn array_at_path<'a>(batch: &'a RecordBatch, path: &str) -> Result<&'a dyn Array> {
  let mut segments = path.split('.');
  let first = segments.next().context("empty Arrow column path")?;
  let mut array = batch
    .column_by_name(first)
    .with_context(|| format!("missing Arrow column '{first}'"))?
    .as_ref();
  for segment in segments {
    let struct_array = array
      .as_any()
      .downcast_ref::<StructArray>()
      .with_context(|| format!("Arrow column before '{segment}' is not a struct"))?;
    array = struct_array
      .column_by_name(segment)
      .with_context(|| format!("missing Arrow struct field '{segment}'"))?
      .as_ref();
  }
  Ok(array)
}

pub(crate) fn read_row_groups(
  file: &LoadedDatasetFile,
  mut visit_batch: impl FnMut(usize, u64, &RecordBatch),
) -> Result<()> {
  for row_group in 0..file.metadata.metadata().num_row_groups() {
    let input = File::open(&file.file.path)
      .with_context(|| format!("open parquet file: {}", file.file.path.display()))?;
    let reader = ParquetRecordBatchReaderBuilder::new_with_metadata(input, file.metadata.clone())
      .with_row_groups(vec![row_group])
      .with_batch_size(1024)
      .build()
      .with_context(|| format!("build parquet reader: {}", file.file.path.display()))?;
    let mut row_offset = 0_u64;
    for batch in reader {
      let batch =
        batch.with_context(|| format!("read parquet rows: {}", file.file.path.display()))?;
      visit_batch(row_group, row_offset, &batch);
      row_offset += batch.num_rows() as u64;
    }
  }
  Ok(())
}
