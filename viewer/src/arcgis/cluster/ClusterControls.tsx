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

import { createPortal } from "react-dom";

import styles from "./ClusterControls.module.css";
import type { ClusterLevel } from "./clusterLevelCatalog";
import type { ClusterModeStatus } from "./useClusterMode";

interface ClusterControlsProps {
  headerActionsElement: HTMLElement | null;
  levels: readonly ClusterLevel[];
  onLevelSelect(level: number): void;
  selectedLevel: number | null;
  status: ClusterModeStatus;
}

export const ClusterControls = ({
  headerActionsElement,
  levels,
  onLevelSelect,
  selectedLevel,
  status,
}: ClusterControlsProps) => {
  if (!headerActionsElement) {
    return null;
  }

  return createPortal(
    <calcite-label className={styles.controls} layout="inline">
      Level
      <calcite-select
        disabled={levels.length === 0}
        label="Multiscale level"
        value={selectedLevel === null ? "" : String(selectedLevel)}
        oncalciteSelectChange={(event: Event) => {
          onLevelSelect(
            Number((event.currentTarget as HTMLCalciteSelectElement).value),
          );
        }}
      >
        {levels.length > 0 ? (
          levels.map((level) => (
            <calcite-option key={level.level} value={String(level.level)}>
              {level.label}
            </calcite-option>
          ))
        ) : (
          <calcite-option value="">No multiscale levels</calcite-option>
        )}
      </calcite-select>
      {status.type === "loading" ? (
        <calcite-loader inline label="Loading Cluster level" scale="s" />
      ) : null}
      {status.type === "error" ? (
        <span className={styles.error} role="status" title={status.error.message}>
          Cluster unavailable
        </span>
      ) : null}
    </calcite-label>,
    headerActionsElement,
  );
};
