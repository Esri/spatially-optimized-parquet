//! Serializes typed metadata into reserved Parquet key-value entries.

use ::parquet::file::metadata::KeyValue;
use anyhow::Result;
use serde::Serialize;

pub(super) trait ParquetMetadata: Serialize {
  const KEY: &'static str;

  fn key_value(&self) -> Result<KeyValue> {
    Ok(KeyValue {
      key: Self::KEY.to_string(),
      value: Some(serde_json::to_string(self)?),
    })
  }
}

pub(super) struct ParquetMetadataSet {
  entries: Vec<KeyValue>,
}

impl ParquetMetadataSet {
  pub(super) fn new(entries: Vec<KeyValue>) -> Self {
    Self { entries }
  }

  pub(super) fn insert<T: ParquetMetadata>(&mut self, metadata: &T) -> Result<()> {
    self.entries.retain(|item| item.key != T::KEY);
    self.entries.push(metadata.key_value()?);
    Ok(())
  }

  pub(super) fn into_entries(self) -> Vec<KeyValue> {
    self.entries
  }
}
