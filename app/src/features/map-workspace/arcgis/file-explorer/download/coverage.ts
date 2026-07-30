import { ParquetByteCoverage } from "../../../../../parquet/byteCoverage";
import type {
  ByteRange,
  FileLayout,
} from "../../../../../parquet/fileLayout";

export class ParquetDownloadCoverage {
  private coverage = new ParquetByteCoverage();
  private downloadedByteLength = 0;

  reset(): void {
    this.coverage = new ParquetByteCoverage();
    this.downloadedByteLength = 0;
  }

  add(range: ByteRange): { addedByteLength: number; newRanges: ByteRange[] } {
    const result = this.coverage.add(range);
    this.downloadedByteLength += result.addedByteLength;
    return result;
  }

  values(): readonly ByteRange[] {
    return this.coverage.values();
  }

  overlaps(range: ByteRange): boolean {
    return this.coverage.overlaps(range);
  }

  currentDownloadedByteLength(): number {
    return this.downloadedByteLength;
  }

  createFileStructureSnapshot(layout: FileLayout | null): {
    layout: FileLayout;
    coverage: ParquetByteCoverage;
  } | null {
    return layout
      ? { layout, coverage: this.coverage.clone() }
      : null;
  }
}
