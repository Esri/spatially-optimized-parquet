use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;
use std::path::PathBuf;

use crate::parquet_dataset::{PartitionDescriptor, PartitionFamily};

use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileCodeRange {
  pub(crate) file: PathBuf,
  pub(crate) family: PartitionFamily,
  pub(crate) minimum: u64,
  pub(crate) maximum: u64,
  pub(crate) partition: Option<PartitionDescriptor>,
}

impl FileCodeRange {
  pub(crate) fn validate_dataset(ranges: &[Self], report: &mut ValidationReport) {
    Self::validate_family(ranges, report);
    Self::validate_partition_bounds(ranges, report);
    Self::warn_overlaps(ranges, report);
  }

  fn validate_family(ranges: &[Self], report: &mut ValidationReport) {
    let mut families = BTreeSet::new();
    for range in ranges {
      families.insert(range.family);
    }
    if families.len() > 1 {
      report.push(
        ValidationRule::DatasetConsistency,
        ValidationSeverity::Error,
        ValidationLocation::default(),
        "dataset mixes Z and XZ clustering families",
      );
    }
  }

  fn validate_partition_bounds(ranges: &[Self], report: &mut ValidationReport) {
    let mut lower_bounds = BTreeMap::<PartitionFamily, BTreeSet<u64>>::new();
    for range in ranges {
      if let Some(partition) = range.partition
        && partition.family == range.family
      {
        lower_bounds
          .entry(range.family)
          .or_default()
          .insert(partition.lower_bound);
      }
    }

    for range in ranges {
      let Some(partition) = range.partition else {
        continue;
      };
      if partition.family != range.family {
        continue;
      }
      if range.minimum < partition.lower_bound {
        report.push(
          ValidationRule::Partition,
          ValidationSeverity::Error,
          ValidationLocation::file(range.file.clone()),
          format!(
            "observed clustering code {} falls below declared partition lower bound {}",
            range.minimum, partition.lower_bound
          ),
        );
      }
      let next_bound = lower_bounds.get(&range.family).and_then(|bounds| {
        bounds
          .range((Bound::Excluded(partition.lower_bound), Bound::Unbounded))
          .next()
          .copied()
      });
      if let Some(next_bound) = next_bound
        && range.maximum >= next_bound
      {
        report.push(
          ValidationRule::Partition,
          ValidationSeverity::Error,
          ValidationLocation::file(range.file.clone()),
          format!(
            "observed clustering code {} reaches the next partition lower bound {}",
            range.maximum, next_bound
          ),
        );
      }
    }
  }

  fn warn_overlaps(ranges: &[Self], report: &mut ValidationReport) {
    for family in [PartitionFamily::Z, PartitionFamily::Xz] {
      let mut family_ranges = ranges
        .iter()
        .filter(|range| range.family == family)
        .collect::<Vec<_>>();
      family_ranges.sort_by_key(|range| (range.minimum, range.maximum, &range.file));
      let mut maximum = None::<(u64, &PathBuf)>;
      for range in family_ranges {
        if let Some((previous_maximum, previous_file)) = maximum
          && range.minimum <= previous_maximum
        {
          report.push(
            ValidationRule::RangeOverlap,
            ValidationSeverity::Warning,
            ValidationLocation::file(range.file.clone()),
            format!(
              "clustering range {}..={} overlaps {}",
              range.minimum,
              range.maximum,
              previous_file.display()
            ),
          );
        }
        if maximum.is_none_or(|(previous_maximum, _)| range.maximum > previous_maximum) {
          maximum = Some((range.maximum, &range.file));
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn multifile_warns_for_overlapping_ranges() {
    let ranges = vec![
      FileCodeRange {
        file: PathBuf::from("a.parquet"),
        family: PartitionFamily::Z,
        minimum: 0,
        maximum: 10,
        partition: None,
      },
      FileCodeRange {
        file: PathBuf::from("b.parquet"),
        family: PartitionFamily::Z,
        minimum: 10,
        maximum: 20,
        partition: None,
      },
    ];
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    FileCodeRange::validate_dataset(&ranges, &mut report);

    assert_eq!(report.warning_count(), 1);
    assert_eq!(report.findings()[0].rule(), ValidationRule::RangeOverlap);
  }
}
