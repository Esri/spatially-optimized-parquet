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

import ClassBreaksRenderer from "@arcgis/core/renderers/ClassBreaksRenderer";
import SimpleFillSymbol from "@arcgis/core/symbols/SimpleFillSymbol";

import type { ParquetFileDiagnostics } from "../../parquet/fileLayout";
import { resolveParquetPageIndexSource } from "../file-explorer/inspector/parquetPageIndexes";
import type { ClusterLevel } from "./clusterLevelCatalog";
import { createClusterPageValueExpression } from "./clusterPageExpression";
import { loadClusterPageTopology } from "./clusterPageTopology";

const clusterColors = [
  "#e60049ff",
  "#0bb4ffff",
  "#50e991ff",
  "#e6d800ff",
  "#9b19f5ff",
  "#ffa300ff",
  "#dc0ab4ff",
  "#b3d4ffff",
  "#00bfa0ff",
  "#f0ccccff",
];

export class ClusterPageRendererStore {
  private readonly cache = new Map<number, Promise<ClassBreaksRenderer>>();

  constructor(
    private readonly source: unknown,
    private readonly files: readonly ParquetFileDiagnostics[],
    private readonly objectIdField: string,
  ) {}

  load(level: ClusterLevel): Promise<ClassBreaksRenderer> {
    const cached = this.cache.get(level.level);
    if (cached) {
      return cached;
    }

    const request = this.create(level).catch((error: unknown) => {
      this.cache.delete(level.level);
      throw error;
    });
    this.cache.set(level.level, request);
    return request;
  }

  private async create(level: ClusterLevel): Promise<ClassBreaksRenderer> {
    const topology = await loadClusterPageTopology(
      resolveParquetPageIndexSource(this.source),
      this.files,
      level,
    );
    return new ClassBreaksRenderer({
      valueExpression: createClusterPageValueExpression(
        topology,
        this.objectIdField,
        clusterColors.length,
      ),
      classBreakInfos: clusterColors.map((color, pageClass) => ({
        minValue: pageClass - 0.5,
        maxValue: pageClass + 0.5,
        symbol: new SimpleFillSymbol({
          color: createDarkFillColor(color),
          outline: { color, width: 1 },
        }),
      })),
    });
  }
}

function createDarkFillColor(color: string): [number, number, number, number] {
  const red = Number.parseInt(color.slice(1, 3), 16);
  const green = Number.parseInt(color.slice(3, 5), 16);
  const blue = Number.parseInt(color.slice(5, 7), 16);
  return [
    Math.round(red * 0.35),
    Math.round(green * 0.35),
    Math.round(blue * 0.35),
    0.65,
  ];
}
