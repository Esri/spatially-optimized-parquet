// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
  /** The MapLibre display does not read attribute columns. */
  properties: Record<string, never>;
  geometry: SupportedGeometry;
}

export interface GeoJSONFeatureCollection {
  type: "FeatureCollection";
  features: GeoJSONFeature[];
}
