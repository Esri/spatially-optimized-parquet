import { useRef } from "react";

import { datasets, type Dataset } from "./datasets";
import { formatByteSize } from "./formatByteSize";

interface DatasetSelectionMenuProps {
  activeDataset: Dataset;
  onDatasetSelect(index: number): void;
}

export function DatasetSelectionMenu({
  activeDataset,
  onDatasetSelect,
}: DatasetSelectionMenuProps) {
  const menuItemRef = useRef<HTMLCalciteMenuItemElement>(null);

  const selectDataset = (index: number) => {
    onDatasetSelect(index);
    if (menuItemRef.current) {
      menuItemRef.current.open = false;
    }
  };

  return (
    <calcite-menu
      className="dataset-menu"
      slot="header-actions-start"
      label="Dataset selection"
    >
      <calcite-menu-item
        ref={menuItemRef}
        text={activeDataset.name}
        label={`Selected dataset: ${activeDataset.name}`}
        iconStart="layers"
      >
        <div className="dataset-options" role="menu" slot="submenu-item">
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
              </button>
              <span className="dataset-option-source">
                {dataset.sourceUrl ? (
                  <calcite-link
                    href={dataset.sourceUrl}
                    iconEnd="launch"
                    rel="noopener noreferrer"
                    target="_blank"
                  >
                    {dataset.source}
                  </calcite-link>
                ) : (
                  dataset.source
                )}
              </span>
            </div>
          ))}
        </div>
      </calcite-menu-item>
    </calcite-menu>
  );
}
