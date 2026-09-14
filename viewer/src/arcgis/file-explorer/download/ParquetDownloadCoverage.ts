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

import { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import type {
  ByteRange,
  FileLayout,
} from "../../../parquet/fileLayout";

/**
 * Tracks downloaded Parquet ranges together with their cumulative byte count.
 * It owns the mutable coverage used by a download session and creates isolated snapshots for file inspection.
 */
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
