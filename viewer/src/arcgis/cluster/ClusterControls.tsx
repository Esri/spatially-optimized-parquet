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
