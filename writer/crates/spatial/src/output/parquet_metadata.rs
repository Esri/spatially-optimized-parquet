//! Defines typed metadata that can replace one reserved Parquet key-value entry.

use anyhow::Result;
use parquet::file::metadata::KeyValue;
use serde::Serialize;

/// Serializes one typed metadata contract into a reserved Parquet key-value entry.
pub trait ParquetMetadata: Serialize {
  /// Names the reserved Parquet metadata key.
  const KEY: &'static str;

  /// Serialize this contract into its Parquet key-value representation.
  fn key_value(&self) -> Result<KeyValue> {
    Ok(KeyValue {
      key: Self::KEY.to_string(),
      value: Some(serde_json::to_string(self)?),
    })
  }
}

/// Stores typed spatial metadata alongside preserved Parquet key-value entries.
pub struct ParquetMetadataSet {
  entries: Vec<KeyValue>,
}

impl ParquetMetadataSet {
  /// Construct a metadata set from source entries that may contain reserved keys.
  pub fn new(entries: Vec<KeyValue>) -> Self {
    Self { entries }
  }

  /// Insert one typed metadata contract, replacing its reserved key.
  pub fn insert<T: ParquetMetadata>(&mut self, metadata: &T) -> Result<()> {
    self.entries.retain(|item| item.key != T::KEY);
    self.entries.push(metadata.key_value()?);
    Ok(())
  }

  /// Return the completed Parquet key-value entries.
  pub fn into_entries(self) -> Vec<KeyValue> {
    self.entries
  }
}
