//! Defines the sortable value produced by Z and XZ clustering algorithms.

/// Stores a sortable Z or XZ clustering key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ClusterKey(u64);

impl ClusterKey {
  /// Construct a clustering key from its encoded value.
  pub(crate) const fn new(value: u64) -> Self {
    Self(value)
  }

  /// Return the encoded clustering value.
  pub(crate) const fn value(self) -> u64 {
    self.0
  }
}

impl From<u64> for ClusterKey {
  fn from(value: u64) -> Self {
    Self::new(value)
  }
}

impl From<ClusterKey> for u64 {
  fn from(key: ClusterKey) -> Self {
    key.value()
  }
}

#[cfg(test)]
mod tests {
  use super::ClusterKey;

  #[test]
  fn cluster_key_preserves_conversion_and_ordering() {
    let lower = ClusterKey::from(7);
    let upper = ClusterKey::new(12);

    assert_eq!(u64::from(lower), 7);
    assert!(lower < upper);
  }
}
