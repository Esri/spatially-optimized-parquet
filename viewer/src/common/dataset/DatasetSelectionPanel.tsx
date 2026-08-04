import { useMemo, useState } from "react";

import {
  defaultCustomDatasetUrl,
  defaultPortalItemId,
  defaultPortalUrl,
  datasets,
  type Dataset,
  type DatasetId,
  type PresetDataset,
} from "./datasets";
import {
  DatasetSelectionMenu,
  type DatasetSelectionOption,
} from "./DatasetSelectionMenu";
import { formatByteSize } from "../formatByteSize";
import { formatCompactCount } from "../formatCompactCount";
import { formatInteger } from "../formatNumber";
import styles from "./DatasetSelectionPanel.module.css";

export interface DatasetSelectionMetrics {
  byteSize: number | null;
  compression: string | null;
  featureCount: number | null;
}

export interface CustomDatasetAction {
  error: string | null;
  loading: boolean;
  onCustomUrlSubmit(url: string): void;
  onPortalItemSubmit(portalUrl: string, itemId: string): void;
}

type DatasetSelectionMode =
  | { type: "preset"; datasetId: DatasetId }
  | { type: "custom-url" }
  | { type: "portal-item" };

interface DatasetSelectionPanelProps {
  activeDataset: Dataset;
  compact?: boolean;
  customAction?: CustomDatasetAction;
  metrics: DatasetSelectionMetrics;
  onDatasetSelect(dataset: PresetDataset): void;
  presetDatasets?: readonly PresetDataset[];
  showPresetMetadata?: boolean;
}

const customUrlOptionId = "custom-url";
const portalItemOptionId = "portal-item";

export function DatasetSelectionPanel({
  activeDataset,
  compact = false,
  customAction,
  metrics,
  onDatasetSelect,
  presetDatasets = datasets,
  showPresetMetadata = true,
}: DatasetSelectionPanelProps) {
  const [selectionMode, setSelectionMode] = useState<DatasetSelectionMode>(
    activeDataset.kind === "preset"
      ? { type: "preset", datasetId: activeDataset.id }
      : { type: activeDataset.kind },
  );
  const [customUrl, setCustomUrl] = useState(defaultCustomDatasetUrl);
  const [portalUrl, setPortalUrl] = useState(defaultPortalUrl);
  const [portalItemId, setPortalItemId] = useState(defaultPortalItemId);
  const [validationError, setValidationError] = useState<string | null>(null);
  const options = useMemo(
    () => createSelectionOptions(
      presetDatasets,
      Boolean(customAction),
      showPresetMetadata,
    ),
    [customAction, presetDatasets, showPresetMetadata],
  );
  const activeLabel = selectionMode.type === "preset"
    ? presetDatasets.find(
        (dataset) => dataset.id === selectionMode.datasetId,
      )?.name ?? activeDataset.name
    : selectionMode.type === "custom-url"
      ? "Custom"
      : "Portal Item";
  const selectedPreset = selectionMode.type === "preset"
    ? presetDatasets.find(
        (dataset) => dataset.id === selectionMode.datasetId,
      ) ?? null
    : null;
  const [compressionCodec, compressionRatio] =
    metrics.compression?.split(" ") ?? [];
  const error = validationError ?? customAction?.error ?? null;

  const submitCustomUrl = () => {
    if (!customAction) {
      return;
    }
    try {
      setValidationError(null);
      customAction.onCustomUrlSubmit(customUrl);
    } catch (error) {
      setValidationError(getErrorMessage(error));
    }
  };

  const submitPortalItem = () => {
    if (!customAction) {
      return;
    }
    try {
      setValidationError(null);
      customAction.onPortalItemSubmit(portalUrl, portalItemId);
    } catch (error) {
      setValidationError(getErrorMessage(error));
    }
  };

  const selectOption = (optionId: string) => {
    setValidationError(null);
    if (optionId === customUrlOptionId) {
      setSelectionMode({ type: "custom-url" });
      submitCustomUrl();
      return;
    }
    if (optionId === portalItemOptionId) {
      setSelectionMode({ type: "portal-item" });
      submitPortalItem();
      return;
    }

    const dataset = presetDatasets.find((candidate) => candidate.id === optionId);
    if (!dataset) {
      return;
    }
    setSelectionMode({ type: "preset", datasetId: dataset.id });
    onDatasetSelect(dataset);
  };

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
          activeLabel={activeLabel}
          activeOptionId={getSelectionModeId(selectionMode)}
          onSelect={selectOption}
          options={options}
        />
        {selectionMode.type === "preset" ? (
          <LabeledInput label="URL">
            <calcite-input
              className={styles.datasetUrlInput}
              label="Parquet URL"
              readOnly
              value={selectedPreset?.parquet.url ?? ""}
            />
          </LabeledInput>
        ) : selectionMode.type === "custom-url" ? (
          <LabeledInput label="URL">
            <div className={styles.datasetSourceEditor}>
              <calcite-input
                className={styles.datasetUrlInput}
                label="Parquet URL"
                value={customUrl}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    submitCustomUrl();
                  }
                }}
                oncalciteInputInput={(event: Event) => {
                  setCustomUrl(
                    (event.currentTarget as HTMLCalciteInputElement).value,
                  );
                }}
              />
              <DatasetSubmitButton
                disabled={customAction?.loading ?? false}
                label="Load custom URL"
                onClick={submitCustomUrl}
              />
            </div>
          </LabeledInput>
        ) : (
          <div className={styles.datasetPortalEditor}>
            <LabeledInput label="Portal">
              <calcite-input
                className={styles.datasetUrlInput}
                label="Portal URL"
                value={portalUrl}
                oncalciteInputInput={(event: Event) => {
                  setPortalUrl(
                    (event.currentTarget as HTMLCalciteInputElement).value,
                  );
                }}
              />
            </LabeledInput>
            <LabeledInput label="ID">
              <div className={styles.datasetSourceEditor}>
                <calcite-input
                  className={styles.datasetUrlInput}
                  label="Portal item ID"
                  value={portalItemId}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      submitPortalItem();
                    }
                  }}
                  oncalciteInputInput={(event: Event) => {
                    setPortalItemId(
                      (event.currentTarget as HTMLCalciteInputElement).value,
                    );
                  }}
                />
                <DatasetSubmitButton
                  disabled={customAction?.loading ?? false}
                  label="Load portal item"
                  onClick={submitPortalItem}
                />
              </div>
            </LabeledInput>
          </div>
        )}
        {error ? (
          <span className={styles.datasetSelectionError} role="alert">
            {error}
          </span>
        ) : null}
      </div>
      <div className={styles.datasetSelectionMetrics}>
        <DatasetMetric
          label="Features"
          value={
            metrics.featureCount === null
              ? "—"
              : formatCompactCount(metrics.featureCount)
          }
        />
        <DatasetMetric
          label="Size"
          value={
            metrics.byteSize === null ? "—" : formatByteSize(metrics.byteSize)
          }
        />
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
              activeDataset.source
            )}
          </span>
        </div>
      </div>
    </section>
  );
}

function createSelectionOptions(
  presetDatasets: readonly PresetDataset[],
  includeCustomOptions: boolean,
  showPresetMetadata: boolean,
): DatasetSelectionOption[] {
  const presetOptions: DatasetSelectionOption[] = presetDatasets.map((dataset) => ({
    id: dataset.id,
    metadata: showPresetMetadata
      ? `${formatByteSize(dataset.byteSize)} · ${formatInteger(dataset.count)} features`
      : undefined,
    name: dataset.name,
    source: dataset.source,
  }));

  return includeCustomOptions
    ? [
        ...presetOptions,
        {
          id: customUrlOptionId,
          name: "Custom",
          source: "Load a Parquet file from a custom URL",
        },
        {
          id: portalItemOptionId,
          name: "Portal Item",
          source: "Load a Parquet Feature Layer portal item",
        },
      ]
    : presetOptions;
}

function getSelectionModeId(selectionMode: DatasetSelectionMode): string {
  return selectionMode.type === "preset"
    ? selectionMode.datasetId
    : selectionMode.type;
}

function LabeledInput({
  children,
  label,
}: {
  children: React.ReactNode;
  label: string;
}) {
  return (
    <div className={styles.datasetUrlField}>
      <span>{label}</span>
      {children}
    </div>
  );
}

function DatasetSubmitButton({
  disabled,
  label,
  onClick,
}: {
  disabled: boolean;
  label: string;
  onClick(): void;
}) {
  return (
    <calcite-button
      appearance="solid"
      className={styles.datasetSubmitAction}
      disabled={disabled}
      iconStart="arrow-bold-right"
      label={label}
      onClick={onClick}
      type="button"
    />
  );
}

function DatasetMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className={styles.datasetSelectionField}>
      <span className={styles.datasetSelectionLabel}>{label}</span>
      <span className={styles.datasetSelectionValue}>{value}</span>
    </div>
  );
}

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
