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
