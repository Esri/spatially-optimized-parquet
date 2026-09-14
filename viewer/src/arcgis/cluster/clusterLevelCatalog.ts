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

import type { ParquetFileDiagnostics } from "../../parquet/fileLayout";
import { extractGeodisplayMetadata } from "../../parquet/keyValueMetadata";

export interface ClusterLevelColumn {
  columnIndex: number;
  fieldName: string;
  fileId: number;
  repeated: boolean;
}

export interface ClusterLevel {
  columns: readonly ClusterLevelColumn[];
  label: string;
  level: number;
}

interface FileLevel {
  column: ClusterLevelColumn;
  label: string;
  level: number;
}

export function deriveClusterLevels(
  files: readonly ParquetFileDiagnostics[],
): ClusterLevel[] {
  if (files.length === 0) {
    return [];
  }

  const fileLevels = files.map(deriveFileLevels);
  const commonLevelNumbers = [...fileLevels[0].keys()].filter((level) =>
    fileLevels.every((levels) => levels.has(level)),
  );
  return commonLevelNumbers
    .sort((left, right) => left - right)
    .map((level) => {
      const fileLevel = fileLevels[0].get(level)!;
      const columns = fileLevels.map((levels) => levels.get(level)!.column);
      return {
        columns,
        label: fileLevel.label,
        level,
      };
    });
}

function deriveFileLevels(
  file: ParquetFileDiagnostics,
): Map<number, FileLevel> {
  const metadata = extractGeodisplayMetadata(file.keyValueMetadata);
  const levels = metadata?.value.levels;
  const result = new Map<number, FileLevel>();
  if (!metadata || !Array.isArray(levels)) {
    return result;
  }

  for (const value of levels) {
    const level = readLevelNumber(value);
    const declaredPath = readLevelColumnPath(value);
    if (level === null || !declaredPath || result.has(level)) {
      continue;
    }

    const fieldPath = [...metadata.parentPath, ...declaredPath];
    const column = resolveLevelColumn(file, fieldPath);
    if (!column) {
      continue;
    }
    result.set(level, {
      level,
      label: formatFieldLeafName(fieldPath.join(".")),
      column: {
        columnIndex: column.index,
        fieldName: column.path.join("."),
        fileId: file.fileId,
        repeated: column.maxRepetitionLevel > 0,
      },
    });
  }
  return result;
}

function resolveLevelColumn(
  file: ParquetFileDiagnostics,
  fieldPath: readonly string[],
) {
  const exactColumn = file.columns.find(({ path }) =>
    pathsEqual(path, fieldPath)
  );
  if (exactColumn) {
    return exactColumn;
  }

  return file.columns.find(({ path }) =>
    path.length > fieldPath.length &&
    path.at(-1) === "x" &&
    fieldPath.every((part, index) => path[index] === part)
  );
}

function readLevelColumnPath(value: unknown): string[] | null {
  if (!isRecord(value)) {
    return null;
  }

  const column = value.column;
  if (typeof column === "string" && column.length > 0) {
    return column.split(".");
  }
  return Array.isArray(column) &&
      column.length > 0 &&
      column.every((part) => typeof part === "string" && part.length > 0)
    ? column
    : null;
}

function readLevelNumber(value: unknown): number | null {
  if (!isRecord(value)) {
    return null;
  }
  return typeof value.level === "number" && Number.isInteger(value.level)
    ? value.level
    : null;
}

function formatFieldLeafName(fieldName: string): string {
  return fieldName.split(".").at(-1) ?? fieldName;
}

function pathsEqual(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length &&
    left.every((part, index) => part === right[index]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
