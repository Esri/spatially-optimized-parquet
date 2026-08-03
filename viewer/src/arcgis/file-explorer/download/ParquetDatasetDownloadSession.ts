import type {
  ParquetDiagnosticsSnapshot,
  ParquetFileDiagnostics,
  ArcgisParquetRangeReadEvent,
} from "../../diagnostics";
import {
  deriveDatasetDetailSummary,
  type DatasetDetailSummary,
} from "../../../parquet/fileDetails";
import type { FileLayout } from "../../../parquet/fileLayout";
import type { RowGroupBound } from "../../../common/rowGroupBounds";
import type { DownloadSessionView } from "./types";
import { ParquetDownloadSession } from "./ParquetDownloadSession";

export interface ParquetDownloadFile {
  readonly diagnostics: ParquetFileDiagnostics;
  readonly download: ParquetDownloadSession;
  readonly layout: FileLayout;
  readonly rowGroupBounds: readonly RowGroupBound[];
}

/**
 * Coordinates diagnostics and range events for every Parquet file in one dataset.
 * Maintains independent child sessions because each file owns a separate byte address space.
 */
export class ParquetDatasetDownloadSession {
  private _files: ParquetDownloadFile[] = [];
  private _fileByRangeId = new Map<string, ParquetDownloadFile>();
  private _pendingEvents: ArcgisParquetRangeReadEvent[] = [];
  private _detailSummary: DatasetDetailSummary | null = null;
  private _rowGroupBounds: RowGroupBound[] = [];
  private _aggregateDownload: ParquetDownloadSession | null = null;
  private readonly _statusDownload = new ParquetDownloadSession();

  get files(): readonly ParquetDownloadFile[] {
    return this._files;
  }

  get detailSummary(): DatasetDetailSummary | null {
    return this._detailSummary;
  }

  get rowGroupBounds(): readonly RowGroupBound[] | null {
    return this._rowGroupBounds.length > 0 ? this._rowGroupBounds : null;
  }

  get statusDownload(): ParquetDownloadSession {
    return this._statusDownload;
  }

  get aggregateDownload(): DownloadSessionView {
    return this._aggregateDownload ?? this._statusDownload;
  }

  async loadDiagnostics(snapshot: ParquetDiagnosticsSnapshot): Promise<void> {
    if (snapshot.files.length === 0) {
      const error = new Error("The Parquet diagnostics snapshot contains no files.");
      this.reportError(error);
      throw error;
    }

    const nextFiles: ParquetDownloadFile[] = [];
    const nextFileByRangeId = new Map<string, ParquetDownloadFile>();
    let nextAggregateDownload: ParquetDownloadSession | null = null;
    try {
      for (const diagnostics of snapshot.files) {
        if (nextFileByRangeId.has(diagnostics.fileName)) {
          throw new Error(
            `The Parquet diagnostics snapshot contains duplicate fileName "${diagnostics.fileName}".`,
          );
        }
        const download = new ParquetDownloadSession();
        try {
          download.loadDiagnostics(snapshot, diagnostics);
        } catch (error) {
          download.dispose();
          throw error;
        }
        const topology = download.topology.getSnapshot();
        if (!topology.layout || !topology.rowGroupBounds) {
          throw new Error(
            `Failed to initialize diagnostics for "${diagnostics.fileName}".`,
          );
        }
        const file: ParquetDownloadFile = {
          diagnostics,
          download,
          layout: topology.layout,
          rowGroupBounds: topology.rowGroupBounds,
        };
        nextFiles.push(file);
        nextFileByRangeId.set(diagnostics.fileName, file);
      }
      nextAggregateDownload = new ParquetDownloadSession();
      nextAggregateDownload.loadDatasetLayouts(
        nextFiles.map(({ layout }) => layout),
        nextFiles.flatMap(({ rowGroupBounds }) => rowGroupBounds),
      );
    } catch (error) {
      nextAggregateDownload?.dispose();
      for (const file of nextFiles) {
        file.download.dispose();
      }
      const diagnosticsError = error instanceof Error
        ? error
        : new Error("Failed to initialize Parquet dataset diagnostics.");
      this.reportError(diagnosticsError);
      throw diagnosticsError;
    }

    this._disposeAggregateDownload();
    this._disposeFiles();
    this._statusDownload.reset();
    this._files = nextFiles;
    this._fileByRangeId = nextFileByRangeId;
    this._detailSummary = deriveDatasetDetailSummary(
      nextFiles.map(({ layout }) => layout),
    );
    this._rowGroupBounds = nextFiles.flatMap(({ rowGroupBounds }) => rowGroupBounds);
    this._aggregateDownload = nextAggregateDownload;

    const pendingEvents = this._pendingEvents;
    this._pendingEvents = [];
    for (const event of pendingEvents) {
      this.recordRangeRead(event);
    }
  }

  recordRangeRead(event: ArcgisParquetRangeReadEvent): void {
    if (this._files.length === 0) {
      this._pendingEvents.push(event);
      return;
    }

    const file = this._fileByRangeId.get(event.fileId);
    if (!file) {
      this.reportError(
        new Error(`Range event references unknown diagnostics file "${event.fileId}".`),
      );
      return;
    }
    this._aggregateDownload?.recordRangeRead(event);
    file.download.recordRangeRead(event);
  }

  reportError(error: Error): void {
    this._statusDownload.reportError(error);
    this._aggregateDownload?.reportError(error);
    for (const file of this._files) {
      file.download.reportError(error);
    }
  }

  reset(): void {
    this._disposeAggregateDownload();
    this._disposeFiles();
    this._files = [];
    this._fileByRangeId.clear();
    this._pendingEvents = [];
    this._detailSummary = null;
    this._rowGroupBounds = [];
    this._statusDownload.reset();
  }

  dispose(): void {
    this.reset();
  }

  private _disposeFiles(): void {
    for (const file of this._files) {
      file.download.dispose();
    }
  }

  private _disposeAggregateDownload(): void {
    this._aggregateDownload?.dispose();
    this._aggregateDownload = null;
  }
}
