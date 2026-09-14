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

import { describe, expect, it } from "vitest";

import { ParquetByteCoverage } from "./ParquetByteCoverage";

describe("ParquetByteCoverage", () => {
  it("merges overlaps while preserving clone isolation", () => {
    const coverage = new ParquetByteCoverage();
    coverage.add({ start: 0, end: 10 });
    coverage.add({ start: 5, end: 15 });
    const clone = coverage.clone();
    clone.add({ start: 20, end: 30 });

    expect(coverage.values).toEqual([{ start: 0, end: 15 }]);
    expect(clone.values).toEqual([
      { start: 0, end: 15 },
      { start: 20, end: 30 },
    ]);
    expect(coverage.state({ start: 0, end: 20 })).toBe("partial");
  });
});
