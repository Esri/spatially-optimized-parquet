import {
  type ByteRange,
  type ColumnStatisticValue,
  type FileLayout,
  rangesOverlap,
} from "./fileLayout";

export type DownloadTrackKind = "column" | "page-index" | "footer";

export interface DownloadPhysicalPiece {
  physicalRange: ByteRange;
  logicalRange: ByteRange;
  segmentId: string;
}

export interface DownloadPhysicalSegment extends DownloadPhysicalPiece {
  trackId: string;
  rowGroupIndex: number | null;
  fileId?: number;
  fileName?: string;
  sourceRowGroupIndex?: number;
  detailLabel: string | null;
  minimumValue: ColumnStatisticValue | null;
  maximumValue: ColumnStatisticValue | null;
  nullCount: number | null;
  recordCount: number | null;
}

export interface DownloadSubpartLayout {
  index: number;
  logicalRange: ByteRange;
  physicalPieces: readonly DownloadPhysicalPiece[];
}

export interface DownloadBlockLayout {
  id: string;
  logicalRange: ByteRange;
  physicalPieces: readonly DownloadPhysicalPiece[];
  subparts: readonly DownloadSubpartLayout[];
}

export interface DownloadTrackLayout {
  id: string;
  label: string;
  kind: DownloadTrackKind;
  byteLength: number;
  blocks: readonly DownloadBlockLayout[];
  segments: readonly DownloadPhysicalSegment[];
}

export interface DownloadDisplayLayout {
  tracks: readonly DownloadTrackLayout[];
  segments: readonly DownloadPhysicalSegment[];
}

interface SourceSegment {
  id: string;
  physicalRange: ByteRange;
  rowGroupIndex: number | null;
  fileId?: number;
  fileName?: string;
  sourceRowGroupIndex?: number;
  detailLabel: string | null;
  minimumValue: ColumnStatisticValue | null;
  maximumValue: ColumnStatisticValue | null;
  nullCount: number | null;
  recordCount: number | null;
}

export interface DatasetDownloadDisplayLayout {
  byteLength: number;
  displayLayout: DownloadDisplayLayout;
  fileOffsets: ReadonlyMap<string, number>;
}

export const displayBlockByteLength = 2 * 1024 * 1024;
export const displaySubpartCount = 16;

export function createDownloadDisplayLayout(layout: FileLayout): DownloadDisplayLayout {
  const tracks: DownloadTrackLayout[] = [];
  const segments: DownloadPhysicalSegment[] = [];
  const columns = new Map<string, SourceSegment[]>();

  for (const rowGroup of layout.rowGroups) {
    for (const column of rowGroup.columns) {
      const sourceSegments = columns.get(column.fieldName) ?? [];
      sourceSegments.push({
        id: column.id,
        physicalRange: column.byteRange,
        rowGroupIndex: rowGroup.index,
        detailLabel: null,
        minimumValue: column.minimumValue,
        maximumValue: column.maximumValue,
        nullCount: column.nullCount,
        recordCount: column.recordCount,
      });
      columns.set(column.fieldName, sourceSegments);
    }
  }

  for (const [fieldName, sourceSegments] of columns) {
    const track = createDownloadTrack(`column:${fieldName}`, fieldName, "column", sourceSegments);
    tracks.push(track.track);
    appendSegments(segments, track.segments);
  }

  const indexSegments = layout.pageIndexes.map((pageIndex) => ({
    id: pageIndex.id,
    physicalRange: pageIndex.byteRange,
    rowGroupIndex: pageIndex.rowGroupIndex,
    detailLabel: pageIndex.fieldName,
    minimumValue: pageIndex.minimumValue,
    maximumValue: pageIndex.maximumValue,
    nullCount: pageIndex.nullCount,
    recordCount: pageIndex.recordCount,
  }));
  if (indexSegments.length > 0) {
    const track = createDownloadTrack("page-index", "Page Index", "page-index", indexSegments);
    tracks.push(track.track);
    appendSegments(segments, track.segments);
  }

  const footer = createDownloadTrack("footer", "Footer", "footer", [{
    id: "footer",
    physicalRange: layout.footer,
    rowGroupIndex: null,
    detailLabel: null,
    minimumValue: null,
    maximumValue: null,
    nullCount: null,
    recordCount: null,
  }]);
  tracks.push(footer.track);
  appendSegments(segments, footer.segments);

  return { tracks, segments };
}

export function createDatasetDownloadDisplayLayout(
  layouts: readonly FileLayout[],
): DatasetDownloadDisplayLayout {
  const tracks: DownloadTrackLayout[] = [];
  const segments: DownloadPhysicalSegment[] = [];
  const columns = new Map<string, SourceSegment[]>();
  const indexSegments: SourceSegment[] = [];
  const footerSegments: SourceSegment[] = [];
  const fileOffsets = new Map<string, number>();
  const rowGroupIndexByFile = new Map<string, number>();
  let fileOffset = 0;
  let aggregateRowGroupIndex = 0;

  for (const layout of layouts) {
    fileOffsets.set(layout.fileName, fileOffset);
    for (const rowGroup of layout.rowGroups) {
      rowGroupIndexByFile.set(
        createRowGroupKey(layout.fileId, rowGroup.index),
        aggregateRowGroupIndex,
      );
      for (const column of rowGroup.columns) {
        const sourceSegments = columns.get(column.fieldName) ?? [];
        sourceSegments.push({
          id: `file${layout.fileId}-${column.id}`,
          physicalRange: shiftRange(column.byteRange, fileOffset),
          rowGroupIndex: aggregateRowGroupIndex,
          fileId: layout.fileId,
          fileName: layout.fileName,
          sourceRowGroupIndex: rowGroup.index,
          detailLabel: null,
          minimumValue: column.minimumValue,
          maximumValue: column.maximumValue,
          nullCount: column.nullCount,
          recordCount: column.recordCount,
        });
        columns.set(column.fieldName, sourceSegments);
      }
      aggregateRowGroupIndex += 1;
    }

    for (const pageIndex of layout.pageIndexes) {
      const rowGroupIndex = rowGroupIndexByFile.get(
        createRowGroupKey(layout.fileId, pageIndex.rowGroupIndex),
      );
      if (rowGroupIndex === undefined) {
        throw new Error(
          `Row group ${pageIndex.rowGroupIndex} is missing from diagnostics file ${layout.fileId}.`,
        );
      }
      indexSegments.push({
        id: `file${layout.fileId}-${pageIndex.id}`,
        physicalRange: shiftRange(pageIndex.byteRange, fileOffset),
        rowGroupIndex,
        fileId: layout.fileId,
        fileName: layout.fileName,
        sourceRowGroupIndex: pageIndex.rowGroupIndex,
        detailLabel: pageIndex.fieldName,
        minimumValue: pageIndex.minimumValue,
        maximumValue: pageIndex.maximumValue,
        nullCount: pageIndex.nullCount,
        recordCount: pageIndex.recordCount,
      });
    }
    footerSegments.push({
      id: `file${layout.fileId}-footer`,
      physicalRange: shiftRange(layout.footer, fileOffset),
      rowGroupIndex: null,
      fileId: layout.fileId,
      fileName: layout.fileName,
      detailLabel: layout.fileName,
      minimumValue: null,
      maximumValue: null,
      nullCount: null,
      recordCount: null,
    });
    fileOffset += layout.byteLength;
  }

  for (const [fieldName, sourceSegments] of columns) {
    const track = createDownloadTrack(
      `column:${fieldName}`,
      fieldName,
      "column",
      sourceSegments,
    );
    tracks.push(track.track);
    appendSegments(segments, track.segments);
  }
  if (indexSegments.length > 0) {
    const track = createDownloadTrack(
      "page-index",
      "Page Index",
      "page-index",
      indexSegments,
    );
    tracks.push(track.track);
    appendSegments(segments, track.segments);
  }
  const footer = createDownloadTrack(
    "footer",
    "Footer",
    "footer",
    footerSegments,
  );
  tracks.push(footer.track);
  appendSegments(segments, footer.segments);

  return {
    byteLength: fileOffset,
    displayLayout: { tracks, segments },
    fileOffsets,
  };
}

function appendSegments(
  target: DownloadPhysicalSegment[],
  source: readonly DownloadPhysicalSegment[],
): void {
  for (const segment of source) {
    target.push(segment);
  }
}

function createDownloadTrack(
  id: string,
  label: string,
  kind: DownloadTrackKind,
  sourceSegments: readonly SourceSegment[],
): { track: DownloadTrackLayout; segments: DownloadPhysicalSegment[] } {
  let logicalStart = 0;
  const segments = sourceSegments.map((sourceSegment) => {
    const byteLength = sourceSegment.physicalRange.end - sourceSegment.physicalRange.start;
    const segment: DownloadPhysicalSegment = {
      ...sourceSegment,
      segmentId: sourceSegment.id,
      trackId: id,
      logicalRange: { start: logicalStart, end: logicalStart + byteLength },
    };
    logicalStart += byteLength;
    return segment;
  });

  return {
    track: {
      id,
      label,
      kind,
      byteLength: logicalStart,
      blocks: createBlocks(id, logicalStart, segments),
      segments,
    },
    segments,
  };
}

function createBlocks(
  trackId: string,
  byteLength: number,
  segments: readonly DownloadPhysicalSegment[],
): DownloadBlockLayout[] {
  const blocks: DownloadBlockLayout[] = [];

  for (let logicalStart = 0, index = 0; logicalStart < byteLength; logicalStart += displayBlockByteLength, index += 1) {
    const logicalRange = {
      start: logicalStart,
      end: Math.min(logicalStart + displayBlockByteLength, byteLength),
    };
    const physicalPieces = clipPieces(logicalRange, segments);
    blocks.push({
      id: `${trackId}:block:${index}`,
      logicalRange,
      physicalPieces,
      subparts: createSubparts(logicalRange, physicalPieces),
    });
  }

  return blocks;
}

function createSubparts(
  blockRange: ByteRange,
  blockPieces: readonly DownloadPhysicalPiece[],
): DownloadSubpartLayout[] {
  const byteLength = blockRange.end - blockRange.start;
  const subparts: DownloadSubpartLayout[] = [];

  for (let index = 0; index < displaySubpartCount; index += 1) {
    const start = blockRange.start + Math.floor((byteLength * index) / displaySubpartCount);
    const end = blockRange.start + Math.floor((byteLength * (index + 1)) / displaySubpartCount);
    const logicalRange = { start, end };
    subparts.push({
      index,
      logicalRange,
      physicalPieces: start < end ? clipPieces(logicalRange, blockPieces) : [],
    });
  }

  return subparts;
}

function clipPieces(
  targetRange: ByteRange,
  pieces: readonly DownloadPhysicalPiece[],
): DownloadPhysicalPiece[] {
  return pieces.flatMap((piece) => {
    if (!rangesOverlap(targetRange, piece.logicalRange)) {
      return [];
    }

    const logicalRange = {
      start: Math.max(targetRange.start, piece.logicalRange.start),
      end: Math.min(targetRange.end, piece.logicalRange.end),
    };
    const physicalOffset = logicalRange.start - piece.logicalRange.start;
    return [{
      logicalRange,
      physicalRange: {
        start: piece.physicalRange.start + physicalOffset,
        end: piece.physicalRange.start + physicalOffset + logicalRange.end - logicalRange.start,
      },
      segmentId: piece.segmentId,
    }];
  });
}

function shiftRange(range: ByteRange, offset: number): ByteRange {
  return {
    start: range.start + offset,
    end: range.end + offset,
  };
}

function createRowGroupKey(fileId: number, rowGroupIndex: number): string {
  return `${fileId}:${rowGroupIndex}`;
}
