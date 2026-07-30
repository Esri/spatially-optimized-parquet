import { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import type {
  ByteRange,
  FileLayout,
} from "../../../parquet/fileLayout";

export class ParquetDownloadCoverage {
  private _coverage = new ParquetByteCoverage();
  private _downloadedByteLength = 0;

  reset(): void {
    this._coverage = new ParquetByteCoverage();
    this._downloadedByteLength = 0;
  }

  add(range: ByteRange): { addedByteLength: number; newRanges: ByteRange[] } {
    const result = this._coverage.add(range);
    this._downloadedByteLength += result.addedByteLength;
    return result;
  }

  get values(): readonly ByteRange[] {
    return this._coverage.values;
  }

  overlaps(range: ByteRange): boolean {
    return this._coverage.overlaps(range);
  }

  get downloadedByteLength(): number {
    return this._downloadedByteLength;
  }

  createFileStructureSnapshot(layout: FileLayout | null): {
    layout: FileLayout;
    coverage: ParquetByteCoverage;
  } | null {
    return layout
      ? { layout, coverage: this._coverage.clone() }
      : null;
  }
}
