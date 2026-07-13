//! Exposes optimized geodisplay metadata contracts and writing.

mod types;
mod writer;

pub use types::{
  DisplayIndex, DisplayIndexXz, DisplayIndexXzInput, DisplayIndexZ, DisplayIndexZInput,
  GeodisplayMetadata,
};
pub(crate) use writer::build_optimized_metadata;
