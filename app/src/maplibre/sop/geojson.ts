/**
 * The GeoJSON model defines the data that `MapLibreParquetLayer` sends to MapLibre.
 * The model supports only XY line and polygon geometry from `spec/display-optimization.md#encoding`.
 * MapLibre applies visual styles after it receives the collection.
 */
export type Position = [number, number];

export interface LineString {
  type: "LineString";
  coordinates: Position[];
}

export interface MultiLineString {
  type: "MultiLineString";
  coordinates: Position[][];
}

export interface Polygon {
  type: "Polygon";
  coordinates: Position[][];
}

export interface MultiPolygon {
  type: "MultiPolygon";
  coordinates: Position[][][];
}

export type SupportedGeometry =
  | LineString
  | MultiLineString
  | Polygon
  | MultiPolygon;

export interface GeoJSONFeature {
  type: "Feature";
  /** The global Parquet row number provides stable identity within one file. */
  id: number;
  /** The MapLibre display does not read attribute columns. */
  properties: Record<string, never>;
  geometry: SupportedGeometry;
}

export interface GeoJSONFeatureCollection {
  type: "FeatureCollection";
  features: GeoJSONFeature[];
}
