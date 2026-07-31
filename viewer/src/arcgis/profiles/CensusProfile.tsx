import DotDensityRenderer from "@arcgis/core/renderers/DotDensityRenderer";
import {
  type Dispatch,
  type SetStateAction,
  useEffect,
  useState,
} from "react";
import { createPortal } from "react-dom";
import type {
  DatasetMapProfile,
  DatasetMapSlotProps,
} from "./profiles";
import styles from "./CensusProfile.module.css";

const colors = {
  popWhite: "#f23c3f",
  popBlack: "#00b6f1",
  popAIAN: "#ff7fe9",
  popAsian: "#32ef94",
  popNHPI: "#e2c4a5",
  popOther: "#ff6a00",
  popTwo: "#96f7ef",
  popHispanic: "#e8ca0d",
};

const demographicAttributes = [
  { field: "P005003", label: "White", color: colors.popWhite },
  {
    field: "P005004",
    label: "Black or African American",
    color: colors.popBlack,
  },
  {
    field: "P005005",
    label: "American Indian and Alaska Native",
    color: colors.popAIAN,
  },
  { field: "P005006", label: "Asian", color: colors.popAsian },
  {
    field: "P005007",
    label: "Native Hawaiian or Pacific Islander",
    color: colors.popNHPI,
  },
  { field: "P005008", label: "Other", color: colors.popOther },
  { field: "P005009", label: "Two or more", color: colors.popTwo },
  {
    field: "P005010",
    label: "Hispanic or Latino",
    color: colors.popHispanic,
  },
] as const;

const defaultPopulationThreshold = 300;

export function createCensusProfile(): DatasetMapProfile {
  return {
    layerProperties: {
    renderer: createDotDensityRenderer(10),
    popupTemplate: {
      title: "Census block {GEOID}",
      content: [
        {
          type: "fields",
          fieldInfos: [
            {
              fieldName: "POP100",
              label: "Population",
              format: { digitSeparator: true, places: 0 },
            },
            {
              fieldName: "HU100",
              label: "Housing units",
              format: { digitSeparator: true, places: 0 },
            },
            {
              fieldName: "AREALAND",
              label: "Land area (m²)",
              format: { digitSeparator: true, places: 0 },
            },
            ...demographicAttributes.map(({ field, label }) => ({
              fieldName: field,
              label,
              format: { digitSeparator: true, places: 0 },
            })),
          ],
        },
      ],
    },
  },
  mapSlotComponent: function CensusMapControls({
    headerActionsElement,
    layer,
  }: DatasetMapSlotProps) {
    const [compactControlsButton, setCompactControlsButton] =
      useState<HTMLCalciteButtonElement | null>(null);
    const [compactControlsOpen, setCompactControlsOpen] = useState(false);
    const [demographicsEnabled, setDemographicsEnabled] = useState(true);
    const [dotValue, setDotValue] = useState(10);
    const [populationThreshold, setPopulationThreshold] = useState(
      defaultPopulationThreshold,
    );

    useEffect(() => {
      if (!layer) {
        return;
      }

      layer.renderer = demographicsEnabled
        ? createDotDensityRenderer(dotValue)
        : createBoundaryRenderer();
      layer.featureEffect =
        demographicsEnabled && populationThreshold > 0
          ? {
              filter: { where: `P005001 > ${populationThreshold}` },
              includedEffect: "drop-shadow(2px, 2px, 12px)",
              excludedEffect: "grayscale(80%)",
            }
          : null;
    }, [demographicsEnabled, dotValue, layer, populationThreshold]);

    return (
      <>
        <arcgis-legend hidden={!demographicsEnabled} slot="bottom-left" />
        {headerActionsElement
          ? createPortal(
              <>
                <div className={styles.censusMapHeaderControls}>
                  {renderCensusRendererControls({
                    demographicsEnabled,
                    dotValue,
                    populationThreshold,
                    setDemographicsEnabled,
                    setDotValue,
                    setPopulationThreshold,
                    controlIdPrefix: "census-inline",
                    showModeLabel: false,
                  })}
                  <span
                    className={styles.mapHeaderActionDivider}
                    aria-hidden="true"
                  >
                    |
                  </span>
                </div>
                <div className={styles.censusMapCompactControls}>
                  <calcite-button
                    ref={setCompactControlsButton}
                    appearance="transparent"
                    iconStart="sliders-horizontal"
                    kind="neutral"
                    label="Census renderer controls"
                    onClick={() => {
                      requestAnimationFrame(() =>
                        setCompactControlsOpen((open) => !open),
                      );
                    }}
                  />
                  {compactControlsButton ? (
                    <calcite-popover
                      label="Census renderer controls"
                      open={compactControlsOpen}
                      overlayPositioning="fixed"
                      placement="bottom-end"
                      referenceElement={compactControlsButton}
                      oncalcitePopoverClose={() =>
                        setCompactControlsOpen(false)
                      }
                    >
                      <div className={styles.censusMapPopoverControls}>
                        {renderCensusRendererControls({
                          demographicsEnabled,
                          dotValue,
                          populationThreshold,
                          setDemographicsEnabled,
                          setDotValue,
                          setPopulationThreshold,
                          controlIdPrefix: "census-popover",
                          showModeLabel: true,
                        })}
                      </div>
                    </calcite-popover>
                  ) : null}
                  <span
                    className={styles.mapHeaderActionDivider}
                    aria-hidden="true"
                  >
                    |
                  </span>
                </div>
              </>,
              headerActionsElement,
            )
          : null}
      </>
    );
  },
  configureLayer(layer) {
    const censusLayerEffect =
      "drop-shadow(3px, 3px, 8px) bloom(0.15, .25px, .1)";

    layer.effect = censusLayerEffect;

    return () => {
      if (layer.effect === censusLayerEffect) {
        layer.effect = null;
      }
    };
    },
  };
}

function renderCensusRendererControls({
  demographicsEnabled,
  dotValue,
  populationThreshold,
  setDemographicsEnabled,
  setDotValue,
  setPopulationThreshold,
  controlIdPrefix,
  showModeLabel,
}: {
  controlIdPrefix: string;
  demographicsEnabled: boolean;
  dotValue: number;
  populationThreshold: number;
  setDemographicsEnabled: Dispatch<SetStateAction<boolean>>;
  setDotValue: Dispatch<SetStateAction<number>>;
  setPopulationThreshold: Dispatch<SetStateAction<number>>;
  showModeLabel: boolean;
}) {
  const modeButton = (
    <calcite-button
      appearance={showModeLabel ? "solid" : "transparent"}
      iconStart={demographicsEnabled ? "polygon-area" : "layer-points"}
      kind="neutral"
      label={demographicsEnabled ? "Show boundaries" : "Show demographics"}
      onClick={() => setDemographicsEnabled((enabled) => !enabled)}
    >
      {showModeLabel
        ? demographicsEnabled
          ? "Show boundaries"
          : "Show dot density"
        : null}
    </calcite-button>
  );

  return (
    <>
      {showModeLabel ? modeButton : null}
      {demographicsEnabled ? (
        <>
          <label id={`${controlIdPrefix}-pop`}>
            <span>Pop</span>
            <strong>{dotValue}</strong>
            <calcite-slider
              label="Dots per person"
              max={200}
              min={1}
              oncalciteSliderInput={(event: Event) =>
                setDotValue(
                  Number((event.currentTarget as HTMLCalciteSliderElement).value),
                )
              }
              value={dotValue}
            />
          </label>
          <calcite-tooltip referenceElement={`${controlIdPrefix}-pop`}>
            Number of people represented by each dot.
          </calcite-tooltip>
          <label id={`${controlIdPrefix}-min`}>
            <span>Min</span>
            <strong>{populationThreshold}</strong>
            <calcite-slider
              label="Emphasize by population count"
              max={400}
              min={0}
              oncalciteSliderInput={(event: Event) =>
                setPopulationThreshold(
                  Number((event.currentTarget as HTMLCalciteSliderElement).value),
                )
              }
              value={populationThreshold}
            />
          </label>
          <calcite-tooltip referenceElement={`${controlIdPrefix}-min`}>
            Emphasize census blocks above this population count.
          </calcite-tooltip>
        </>
      ) : null}
      {showModeLabel ? null : modeButton}
    </>
  );
}

function createBoundaryRenderer() {
  return {
    type: "simple" as const,
    symbol: {
      type: "simple-fill" as const,
      color: "black",
      outline: {
        color: [255, 255, 255, 0.4],
        width: "1px",
      },
    },
  };
}

function createDotDensityRenderer(dotValue: number): DotDensityRenderer {
  return new DotDensityRenderer({
    dotSize: 3,
    dotValue,
    dotBlendingEnabled: true,
    outline: undefined,
    attributes: demographicAttributes.map((attribute) => ({ ...attribute })),
  });
}
