/// Selects the physical representation for optimized multiscale geometry levels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MultiscaleEncoding {
  /// Encodes each geometry as an Esri PBF byte array.
  #[default]
  Pbf,
  /// Encodes quantized coordinates as nested integer columns.
  QuantizedNative,
}
