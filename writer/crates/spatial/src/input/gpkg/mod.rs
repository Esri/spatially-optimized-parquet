//! Integrates GeoPackage vector layers through GDAL's Arrow C stream interface.
//!
//! [`open_source`] recognizes local `.gpkg` files, opens only the GDAL `GPKG` driver,
//! inventories vector layers, resolves the requested layer, and normalizes geometry metadata
//! and Arrow schema. DataFusion schedules rowid partitions while GDAL owns SQLite access,
//! feature decoding, WKB production, and Arrow conversion.

mod arrow;
mod metadata;
mod open;
mod partition;
mod source;

pub use source::{GpkgInputSource, open_source};
