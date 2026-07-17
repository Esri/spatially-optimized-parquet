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
    self.insert_entry(metadata.key_value()?);
    Ok(())
  }

  pub(super) fn insert_entry(&mut self, entry: KeyValue) {
    self.entries.retain(|item| item.key != entry.key);
    self.entries.push(entry);
  }

  pub(super) fn into_entries(self) -> Vec<KeyValue> {
    self.entries
  }
}
