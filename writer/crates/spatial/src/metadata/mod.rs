//! Defines metadata contracts at the input and output boundaries.
//!
//! [`source`] contains a format-neutral model populated by GeoPackage and Parquet providers.
//! It records geometry column identity, encoding, declared types, extent, CRS, dimensions, and
//! safe pass-through Parquet key-value entries. Analysis and reprojection consume this model
//! without depending on GDAL or GeoParquet parser types.
//!
//! [`output`] contains serializable geodisplay structures written alongside GeoParquet metadata.
//! Keeping these models separate prevents source-format quirks from leaking into the public file
//! contract and makes metadata generation independently testable.

pub mod output;
pub mod source;
