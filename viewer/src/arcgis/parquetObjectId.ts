export type ParquetFileId = number;
export type ParquetRowId = number;
export type ParquetObjectId = number;

const parquetRowIdBits = 32;
const parquetFileIdBits = 16;
const parquetRowIdRange = 2 ** parquetRowIdBits;
const parquetFileIdRange = 2 ** parquetFileIdBits;

export function getParquetFileId(
  objectId: ParquetObjectId,
): ParquetFileId {
  return Math.floor(objectId / parquetRowIdRange) % parquetFileIdRange;
}

export function getParquetRowId(objectId: ParquetObjectId): ParquetRowId {
  return objectId % parquetRowIdRange;
}

export function getParquetObjectId(
  fileId: ParquetFileId,
  rowId: ParquetRowId,
): ParquetObjectId {
  return fileId * parquetRowIdRange + rowId;
}

export function createParquetObjectIdArcadeVariables(
  objectIdField: string,
): string[] {
  return [
    `var objectId = $feature["${escapeArcadeString(objectIdField)}"];`,
    `var fileId = Floor(objectId / ${parquetRowIdRange});`,
    `var rowId = objectId - fileId * ${parquetRowIdRange};`,
  ];
}

function escapeArcadeString(value: string): string {
  return value.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
}
