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
  id: number;
  properties: Record<string, never>;
  geometry: SupportedGeometry;
}

export interface GeoJSONFeatureCollection {
  type: "FeatureCollection";
  features: GeoJSONFeature[];
}
