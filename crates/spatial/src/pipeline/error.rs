use crate::output::OutputError;
use crate::{GeoParquetError, GeometryError, InputError, SessionError};

/// Represents failures while converting one spatial dataset.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
  /// Wraps an input-source failure.
  #[error(transparent)]
  Input(#[from] InputError),
  /// Wraps a geometry operation failure.
  #[error(transparent)]
  Geometry(#[from] GeometryError),
  /// Wraps a GeoParquet metadata or spatial-reference failure.
  #[error(transparent)]
  GeoParquet(#[from] GeoParquetError),
  /// Wraps a DataFusion session setup failure.
  #[error(transparent)]
  Session(#[from] SessionError),
  /// Wraps an output preparation or write failure.
  #[error(transparent)]
  Output(#[from] OutputError),
  /// Reports a DataFusion planning or execution failure.
  #[error("DataFusion pipeline operation {operation} failed: {source}")]
  DataFusion {
    /// Identifies the failed DataFusion operation.
    operation: &'static str,
    /// Preserves the DataFusion error.
    #[source]
    source: datafusion::common::DataFusionError,
  },
  /// Reports invalid pipeline request options.
  #[error("invalid pipeline request: {0}")]
  InvalidRequest(String),
}
