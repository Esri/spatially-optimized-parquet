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
import type {
  ParquetOffsetIndex,
  ParquetPageIndexSource,
} from "../file-explorer/inspector/parquetPageIndexes";
import type { ClusterLevel } from "./clusterLevelCatalog";

export interface ClusterFilePageIndex {
  fileId: number;
  pageStarts: readonly number[];
  rowEnd: number;
}

const maximumConcurrentIndexReads = 16;

export async function loadClusterPageTopology(
  source: ParquetPageIndexSource,
  files: readonly ParquetFileDiagnostics[],
  level: ClusterLevel,
): Promise<ClusterFilePageIndex[]> {
  return Promise.all(
    files.map((file) => loadFilePageIndex(source, file, level)),
  );
}

async function loadFilePageIndex(
  source: ParquetPageIndexSource,
  file: ParquetFileDiagnostics,
  level: ClusterLevel,
): Promise<ClusterFilePageIndex> {
  const column = level.columns.find(({ fileId }) => fileId === file.fileId);
  if (!column) {
    throw new Error(
      `Level ${level.level} does not identify a column in "${file.fileName}".`,
    );
  }

  const offsetIndexes = await mapWithConcurrency(
    file.rowGroups,
    maximumConcurrentIndexReads,
    (rowGroup) =>
      source.getOffsetIndex({
        fileId: file.fileId,
        rowGroupIndex: rowGroup.index,
        columnIndex: column.columnIndex,
      }),
  );
  const pageStarts: number[] = [];
  let rowEnd = 0;
  for (const [rowGroupIndex, rowGroup] of file.rowGroups.entries()) {
    if (rowGroup.rowStart !== rowEnd) {
      throw new Error(
        `Row groups are not contiguous in "${file.fileName}".`,
      );
    }
    const offsetIndex = offsetIndexes[rowGroupIndex];
    validateOffsetIndex(
      file,
      rowGroup.index,
      rowGroup.rowCount,
      offsetIndex,
      column.repeated,
    );
    for (const pageStart of getFeaturePageStarts(
      offsetIndex,
      column.repeated,
    )) {
      pageStarts.push(rowGroup.rowStart + pageStart);
    }
    rowEnd = rowGroup.rowStart + rowGroup.rowCount;
  }

  if (pageStarts.length === 0) {
    throw new Error(
      `Level ${level.level} exposes no pages in "${file.fileName}".`,
    );
  }
  return { fileId: file.fileId, pageStarts, rowEnd };
}

function validateOffsetIndex(
  file: ParquetFileDiagnostics,
  rowGroupIndex: number,
  rowCount: number,
  offsetIndex: ParquetOffsetIndex | null,
  repeated: boolean,
): asserts offsetIndex is ParquetOffsetIndex {
  if (!offsetIndex || offsetIndex.pages.length === 0) {
    throw new Error(
      `Level page offsets are unavailable for row group ${rowGroupIndex} in "${file.fileName}".`,
    );
  }

  if (repeated) {
    validateRepeatedOffsetIndex(file, rowGroupIndex, rowCount, offsetIndex);
    return;
  }

  let expectedRowStart = 0;
  for (const page of offsetIndex.pages) {
    if (page.rowStart !== expectedRowStart || page.rowEnd <= page.rowStart) {
      throw new Error(
        `Level page offsets are not contiguous for row group ${rowGroupIndex} in "${file.fileName}".`,
      );
    }
    expectedRowStart = page.rowEnd;
  }
  if (expectedRowStart !== rowCount) {
    throw new Error(
      `Level page offsets do not cover row group ${rowGroupIndex} in "${file.fileName}".`,
    );
  }
}

function validateRepeatedOffsetIndex(
  file: ParquetFileDiagnostics,
  rowGroupIndex: number,
  rowCount: number,
  offsetIndex: ParquetOffsetIndex,
): void {
  let previousRowStart = -1;
  for (const page of offsetIndex.pages) {
    if (
      page.rowStart < previousRowStart ||
      page.rowStart < 0 ||
      page.rowStart >= rowCount
    ) {
      throw new Error(
        `Level page offsets are invalid for repeated row group ${rowGroupIndex} in "${file.fileName}".`,
      );
    }
    previousRowStart = page.rowStart;
  }
  if (offsetIndex.pages[0].rowStart !== 0) {
    throw new Error(
      `Level page offsets do not start with row group ${rowGroupIndex} in "${file.fileName}".`,
    );
  }
}

function getFeaturePageStarts(
  offsetIndex: ParquetOffsetIndex,
  repeated: boolean,
): number[] {
  if (!repeated) {
    return offsetIndex.pages.map(({ rowStart }) => rowStart);
  }

  return offsetIndex.pages
    .map(({ rowStart }) => rowStart)
    .filter((rowStart, index, values) =>
      index === 0 || rowStart !== values[index - 1]
    );
}

async function mapWithConcurrency<Input, Output>(
  values: readonly Input[],
  concurrency: number,
  load: (value: Input) => Promise<Output>,
): Promise<Output[]> {
  const results = new Array<Output>(values.length);
  let nextIndex = 0;

  const worker = async () => {
    while (nextIndex < values.length) {
      const index = nextIndex++;
      results[index] = await load(values[index]);
    }
  };
  await Promise.all(
    Array.from(
      { length: Math.min(concurrency, values.length) },
      () => worker(),
    ),
  );
  return results;
}
