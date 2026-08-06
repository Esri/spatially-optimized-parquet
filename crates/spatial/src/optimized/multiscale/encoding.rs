/// Selects the physical representation for optimized multiscale geometry levels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MultiscaleEncoding {
  /// Encodes each geometry as an Esri PBF byte array.
  #[default]
  Pbf,
  /// Encodes quantized grid coordinates as ISO WKB byte arrays.
  WkbQuantized,
  /// Encodes snapped world coordinates as ISO WKB byte arrays.
  Wkb,
  /// Encodes quantized coordinates as nested integer columns.
  NativeQuantized,
  /// Encodes quantized grid coordinates as nested floating-point columns.
  NativeQuantizedFloat,
  /// Encodes snapped world coordinates as nested floating-point columns.
  Native,
}
