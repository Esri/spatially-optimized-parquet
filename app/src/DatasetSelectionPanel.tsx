import type { Dataset } from "./datasets";
import { DatasetSelectionMenu } from "./DatasetSelectionMenu";
import { formatByteSize } from "./formatByteSize";

interface DatasetSelectionPanelProps {
  activeDataset: Dataset;
  byteSize?: number;
  compression?: string | null;
  onDatasetSelect(index: number): void;
}

export function DatasetSelectionPanel({
  activeDataset,
  byteSize = activeDataset.byteSize,
  compression = null,
  onDatasetSelect,
}: DatasetSelectionPanelProps) {
  const [compressionCodec, compressionRatio] = compression?.split(" ") ?? [];

  return (
    <section
      aria-label="Dataset selection"
      className="dataset-selection-panel"
    >
      <div className="dataset-selection-main">
        <DatasetSelectionMenu
          activeDataset={activeDataset}
          onDatasetSelect={onDatasetSelect}
        />
        <div className="dataset-url-field">
          <span>URL</span>
          <calcite-input
            className="dataset-url-input"
            label="Parquet URL"
            readOnly
            value={activeDataset.url}
          />
        </div>
      </div>
      <div className="dataset-selection-metrics">
        <div className="dataset-selection-field">
          <span className="dataset-selection-label">Features</span>
          <span className="dataset-selection-value">
            {formatCompactCount(activeDataset.count)}
          </span>
        </div>
        <div className="dataset-selection-field">
          <span className="dataset-selection-label">Size</span>
          <span className="dataset-selection-value">
            {formatByteSize(byteSize)}
          </span>
        </div>
        <div className="dataset-selection-field">
          <span className="dataset-selection-label">Compression</span>
          <span className="dataset-selection-value">
            {compressionCodec ?? "—"}
            {compressionRatio ? (
              <small className="dataset-compression-ratio">
                {compressionRatio}
              </small>
            ) : null}
          </span>
        </div>
        <div className="dataset-selection-field dataset-source-metric">
          <span className="dataset-selection-label">Source</span>
          <span
            className="dataset-selection-value"
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

function formatCompactCount(count: number): string {
  return new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1,
    notation: "compact",
  }).format(count);
}
