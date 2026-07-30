import type { ArcgisParquetKeyValueMetadataV1 } from "./fileLayout";

export function formatParquetKeyValueMetadata(
  metadata: readonly ArcgisParquetKeyValueMetadataV1[],
): string {
  const values = Object.fromEntries(
    metadata.map(({ key, value }) => [key, parseMetadataValue(value)]),
  );
  return JSON.stringify(values, null, 2);
}

export function extractGeodisplayVersion(
  metadata: readonly ArcgisParquetKeyValueMetadataV1[],
): string | null {
  const directValue = metadata.find(({ key }) => key === "geodisplay")?.value;
  const directVersion = directValue
    ? readVersion(parseMetadataValue(directValue))
    : null;
  if (directVersion) {
    return directVersion;
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
    ? readVersion(geodisplayField.metadata)
    : null;
}

export function extractGeoParquetVersion(
  metadata: readonly ArcgisParquetKeyValueMetadataV1[],
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
