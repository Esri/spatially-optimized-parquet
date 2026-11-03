//! Share serialization rules across spatial metadata contracts.

use serde::{Deserialize, Deserializer};

/// Deserialize a present optional field as its value type, rejecting explicit null.
///
/// Pair with `serde(default)` so an omitted field becomes `None`.
pub(crate) fn deserialize_optional_non_null<'de, Value, Decoder>(
  decoder: Decoder,
) -> Result<Option<Value>, Decoder::Error>
where
  Value: Deserialize<'de>,
  Decoder: Deserializer<'de>,
{
  Value::deserialize(decoder).map(Some)
}
