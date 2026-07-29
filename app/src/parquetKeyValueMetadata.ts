import type { ArcgisParquetKeyValueMetadataV1 } from "./arcgisParquetDiagnostics";

export function formatParquetKeyValueMetadata(
  metadata: readonly ArcgisParquetKeyValueMetadataV1[],
): string {
  const values = Object.fromEntries(
    metadata.map(({ key, value }) => [key, parseMetadataValue(value)]),
  );
  return JSON.stringify(values, null, 2);
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
