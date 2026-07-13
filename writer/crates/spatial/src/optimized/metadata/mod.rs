//! Exposes optimized geodisplay metadata contracts and writing.

mod types;
mod writer;

pub use types::{
  ClusteringIndex, GeodisplayMetadata, XzClusteringIndex, XzClusteringIndexInput, ZClusteringIndex,
  ZClusteringIndexInput,
};
