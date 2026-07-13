use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;

use engine::parquet_write::{create_datafusion_parquet_options, parse_compression};

#[test]
fn parse_compression_accepts_known_codecs() {
  let codec = parse_compression("snappy").unwrap();
  assert!(matches!(codec, Compression::SNAPPY));
  let codec = parse_compression("gzip").unwrap();
  assert!(matches!(codec, Compression::GZIP(_)));
  let codec = parse_compression("uncompressed").unwrap();
  assert!(matches!(codec, Compression::UNCOMPRESSED));
}

#[test]
fn parse_compression_rejects_invalid() {
  let err = parse_compression("bogus").unwrap_err();
  assert!(err.to_string().contains("compression"));
}

#[test]
fn create_datafusion_parquet_options_preserves_writer_tuning_and_metadata() {
  let options = create_datafusion_parquet_options(
    Compression::GZIP(parquet::basic::GzipLevel::default()),
    &[KeyValue::new("geo".to_string(), Some("{}".to_string()))],
  );

  assert_eq!(options.global.compression.as_deref(), Some("gzip(6)"));
  assert_eq!(options.global.dictionary_enabled, Some(false));
  assert_eq!(
    options
      .key_value_metadata
      .get("geo")
      .and_then(|value| value.as_deref()),
    Some("{}")
  );
}
