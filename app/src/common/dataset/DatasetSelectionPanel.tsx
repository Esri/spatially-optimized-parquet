import type { Dataset } from "./datasets";
import { DatasetSelectionMenu } from "./DatasetSelectionMenu";
import { formatByteSize } from "../formatByteSize";
import { formatCompactCount } from "../formatCompactCount";
import styles from "./DatasetSelectionPanel.module.css";

interface DatasetSelectionPanelProps {
  activeDataset: Dataset;
  byteSize?: number;
  compression?: string | null;
  compact?: boolean;
  onDatasetSelect(index: number): void;
}

export function DatasetSelectionPanel({
  activeDataset,
  byteSize = activeDataset.byteSize,
  compact = false,
  compression = null,
  onDatasetSelect,
}: DatasetSelectionPanelProps) {
  const [compressionCodec, compressionRatio] = compression?.split(" ") ?? [];

  return (
    <section
      aria-label="Dataset selection"
      className={[
        styles.datasetSelectionPanel,
        compact ? styles.compact : null,
      ].filter(Boolean).join(" ")}
    >
      <div className={styles.datasetSelectionMain}>
        <DatasetSelectionMenu
          activeDataset={activeDataset}
          onDatasetSelect={onDatasetSelect}
        />
        <div className={styles.datasetUrlField}>
          <span>URL</span>
          <calcite-input
            className={styles.datasetUrlInput}
            label="Parquet URL"
            readOnly
            value={activeDataset.url}
          />
        </div>
      </div>
      <div className={styles.datasetSelectionMetrics}>
        <div className={styles.datasetSelectionField}>
          <span className={styles.datasetSelectionLabel}>Features</span>
          <span className={styles.datasetSelectionValue}>
            {formatCompactCount(activeDataset.count)}
          </span>
        </div>
        <div className={styles.datasetSelectionField}>
          <span className={styles.datasetSelectionLabel}>Size</span>
          <span className={styles.datasetSelectionValue}>
            {formatByteSize(byteSize)}
          </span>
        </div>
        <div className={styles.datasetSelectionField}>
          <span className={styles.datasetSelectionLabel}>Compression</span>
          <span className={styles.datasetSelectionValue}>
            {compressionCodec ?? "—"}
            {compressionRatio ? (
              <small className={styles.datasetCompressionRatio}>
                {compressionRatio}
              </small>
            ) : null}
          </span>
        </div>
        <div
          className={`${styles.datasetSelectionField} ${styles.datasetSourceMetric}`}
        >
          <span className={styles.datasetSelectionLabel}>Source</span>
          <span
            className={styles.datasetSelectionValue}
            title={activeDataset.source}
          >
            {activeDataset.sourceUrl ? (
              <calcite-link
                href={activeDataset.sourceUrl}
                iconEnd="launch"
                rel="noopener noreferrer"
                target="_blank"
              >
                Open
              </calcite-link>
            ) : (
              "Unavailable"
            )}
          </span>
        </div>
      </div>
    </section>
  );
}
