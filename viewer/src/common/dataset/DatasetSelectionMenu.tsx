import { useRef, useState } from "react";

import { datasets, type Dataset } from "./datasets";
import { formatByteSize } from "../formatByteSize";
import { formatInteger } from "../formatNumber";
import styles from "./DatasetSelectionPanel.module.css";

interface DatasetSelectionMenuProps {
  activeDataset: Dataset;
  onDatasetSelect(index: number): void;
}

export function DatasetSelectionMenu({
  activeDataset,
  onDatasetSelect,
}: DatasetSelectionMenuProps) {
  const menuButtonRef = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);

  const selectDataset = (index: number) => {
    onDatasetSelect(index);
    setMenuOpen(false);
  };

  return (
    <label
      className={`${styles.datasetSelectionField} ${styles.datasetSelectField}`}
    >
      <span className={styles.datasetSelectionLabel}>Select Dataset</span>
      <button
        aria-expanded={menuOpen}
        aria-haspopup="menu"
        className={styles.datasetMenuTrigger}
        onClick={() => {
          requestAnimationFrame(() => setMenuOpen((open) => !open));
        }}
        ref={menuButtonRef}
        type="button"
      >
        {activeDataset.name}
      </button>
      {menuButtonRef.current ? (
        <calcite-popover
          label="Select dataset"
          open={menuOpen}
          overlayPositioning="fixed"
          placement="bottom-start"
          referenceElement={menuButtonRef.current}
          oncalcitePopoverClose={() => setMenuOpen(false)}
        >
          <div className={styles.datasetOptions} role="menu">
            {datasets.map((dataset, index) => (
              <div className={styles.datasetOption} key={dataset.id} role="none">
                <button
                  className={styles.datasetOptionSelect}
                  disabled={!dataset.url}
                  onClick={() => selectDataset(index)}
                  role="menuitem"
                  type="button"
                >
                  <span className={styles.datasetOptionName}>{dataset.name}</span>
                  <span className={styles.datasetOptionMetadata}>
                    {formatByteSize(dataset.byteSize)} ·{" "}
                    {formatInteger(dataset.count)} features
                  </span>
                  <span className={styles.datasetOptionSource}>
                    {dataset.source}
                  </span>
                </button>
              </div>
            ))}
          </div>
        </calcite-popover>
      ) : null}
    </label>
  );
}
