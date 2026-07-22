//! Resolves state shared by spatial output writers.

use anyhow::{Context, Result};
use arrow_schema::Schema;
use datafusion::dataframe::DataFrame;

use crate::geometry::Extent2D;
use crate::input::{InputSource, RowRange};

use super::{
  ExtentResolver, NormalizedSpatialFrame, ResolvedReprojection, ResolvedSpatialSource,
  resolve_source,
};

/// Owns resolved facts and normalized data for one spatial write.
pub(crate) struct SpatialWriteContext {
  source: ResolvedSpatialSource,
  reprojection: ResolvedReprojection,
  frame: NormalizedSpatialFrame,
  target_extent: Extent2D,
}

impl SpatialWriteContext {
  /// Resolve source facts and normalized data for one spatial write.
  pub(crate) async fn resolve(
    input: &dyn InputSource,
    input_dataframe: DataFrame,
    source_schema: &Schema,
    geometry_column: Option<&str>,
    input_wkid: Option<u32>,
    row_range: RowRange,
    output_wkid: u32,
    strip_z: bool,
    strip_m: bool,
    normalization_extent: Option<[f64; 4]>,
  ) -> Result<Self> {
    let mut source = resolve_source(
      input,
      input_dataframe.clone(),
      source_schema,
      geometry_column,
      input_wkid,
      row_range,
    )
    .await?;
    source.strip_dimensions(strip_z, strip_m);
    let source_projjson = source
      .source_spatial_reference
      .projjson
      .as_ref()
      .context("missing resolved source CRS PROJJSON")?;
    let reprojection = ResolvedReprojection::from_source_projjson(source_projjson, output_wkid)?;
    let frame = NormalizedSpatialFrame::new(
      input_dataframe,
      source_schema,
      &source,
      &reprojection,
      strip_z,
      strip_m,
    )?;
    let target_extent = match normalization_extent {
      Some([xmin, ymin, xmax, ymax]) => Extent2D {
        xmin,
        ymin,
        xmax,
        ymax,
      },
      None => {
        ExtentResolver::new(input, row_range)
          .resolve(&source, &frame, &reprojection)
          .await?
      }
    };
    Ok(Self {
      source,
      reprojection,
      frame,
      target_extent,
    })
  }

  /// Return resolved source facts for the selected rows.
  pub(crate) fn source(&self) -> &ResolvedSpatialSource {
    &self.source
  }

  /// Return the selected output coordinate transformation.
  pub(crate) fn reprojection(&self) -> &ResolvedReprojection {
    &self.reprojection
  }

  /// Return the normalized DataFusion frame.
  pub(crate) fn frame(&self) -> &NormalizedSpatialFrame {
    &self.frame
  }

  /// Return the selected-row extent in output coordinates.
  pub(crate) fn target_extent(&self) -> Extent2D {
    self.target_extent
  }
}
