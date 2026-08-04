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
    validateOffsetIndex(file, rowGroup.index, rowGroup.rowCount, offsetIndex);
    for (const page of offsetIndex.pages) {
      pageStarts.push(rowGroup.rowStart + page.rowStart);
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
): asserts offsetIndex is ParquetOffsetIndex {
  if (!offsetIndex || offsetIndex.pages.length === 0) {
    throw new Error(
      `Level page offsets are unavailable for row group ${rowGroupIndex} in "${file.fileName}".`,
    );
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
