import { useRef, useState } from "react";

import styles from "./DatasetSelectionPanel.module.css";

export interface DatasetSelectionOption {
  id: string;
  metadata?: string;
  name: string;
  source?: string;
}

interface DatasetSelectionMenuProps {
  activeLabel: string;
  onSelect(optionId: string): void;
  options: readonly DatasetSelectionOption[];
}

export function DatasetSelectionMenu({
  activeLabel,
  onSelect,
  options,
}: DatasetSelectionMenuProps) {
  const menuButtonRef = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);

  const selectDataset = (optionId: string) => {
    onSelect(optionId);
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
        {activeLabel}
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
            {options.map((option) => (
              <div className={styles.datasetOption} key={option.id} role="none">
                <button
                  className={styles.datasetOptionSelect}
                  onClick={() => selectDataset(option.id)}
                  role="menuitem"
                  type="button"
                >
                  <span className={styles.datasetOptionName}>{option.name}</span>
                  {option.metadata ? (
                    <span className={styles.datasetOptionMetadata}>
                      {option.metadata}
                    </span>
                  ) : null}
                  {option.source ? (
                    <span className={styles.datasetOptionSource}>
                      {option.source}
                    </span>
                  ) : null}
                </button>
              </div>
            ))}
          </div>
        </calcite-popover>
      ) : null}
    </label>
  );
}
