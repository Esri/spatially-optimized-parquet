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

import type { ParquetKeyValueMetadata } from "./fileLayout";

export interface GeodisplayMetadata {
  parentPath: string[];
  value: Record<string, unknown>;
}

export function formatParquetKeyValueMetadata(
  metadata: readonly ParquetKeyValueMetadata[],
): string {
  const values = Object.fromEntries(
    metadata.map(({ key, value }) => [key, parseMetadataValue(value)]),
  );
  return JSON.stringify(values, null, 2);
}

export function extractGeodisplayVersion(
  metadata: readonly ParquetKeyValueMetadata[],
): string | null {
  return readVersion(extractGeodisplayMetadata(metadata)?.value);
}

export function extractGeodisplayMetadata(
  metadata: readonly ParquetKeyValueMetadata[],
): GeodisplayMetadata | null {
  const directValue = metadata.find(({ key }) => key === "geodisplay")?.value;
  const directMetadata = directValue ? parseMetadataValue(directValue) : null;
  if (isRecord(directMetadata)) {
    return { parentPath: [], value: directMetadata };
  }

  const sparkValue = metadata.find(
    ({ key }) => key === "org.apache.spark.sql.parquet.row.metadata",
  )?.value;
  const sparkMetadata = sparkValue ? parseMetadataValue(sparkValue) : null;
  if (!isRecord(sparkMetadata) || !Array.isArray(sparkMetadata.fields)) {
    return null;
  }
  const geodisplayField = sparkMetadata.fields.find(
    (field) => isRecord(field) && field.name === "geodisplay",
  );
  return isRecord(geodisplayField) && isRecord(geodisplayField.metadata)
    ? { parentPath: ["geodisplay"], value: geodisplayField.metadata }
    : null;
}

export function extractGeoParquetVersion(
  metadata: readonly ParquetKeyValueMetadata[],
): string | null {
  const geoValue = metadata.find(({ key }) => key === "geo")?.value;
  return geoValue ? readVersion(parseMetadataValue(geoValue)) : null;
}

function parseMetadataValue(value: string | null): unknown {
  if (value === null) {
    return null;
  }

  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function readVersion(value: unknown): string | null {
  return isRecord(value) && typeof value.version === "string"
    ? value.version
    : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
