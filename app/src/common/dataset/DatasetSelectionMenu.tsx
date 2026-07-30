import { useRef, useState } from "react";

import { datasets, type Dataset } from "./datasets";
import { formatByteSize } from "../formatByteSize";

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
    <label className="dataset-selection-field dataset-select-field">
      <span className="dataset-selection-label">Select Dataset</span>
      <button
        aria-expanded={menuOpen}
        aria-haspopup="menu"
        className="dataset-menu-trigger"
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
          <div className="dataset-options" role="menu">
            {datasets.map((dataset, index) => (
              <div className="dataset-option" key={dataset.id} role="none">
                <button
                  className="dataset-option-select"
                  disabled={!dataset.url}
                  onClick={() => selectDataset(index)}
                  role="menuitem"
                  type="button"
                >
                  <span className="dataset-option-name">{dataset.name}</span>
                  <span className="dataset-option-metadata">
                    {formatByteSize(dataset.byteSize)} ·{" "}
                    {dataset.count.toLocaleString()} features
                  </span>
                  <span className="dataset-option-source">
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
