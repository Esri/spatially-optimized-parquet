//! Implements ordinary GeoParquet output without spatial optimization.

mod write;

pub(crate) use write::PlainWriter;
