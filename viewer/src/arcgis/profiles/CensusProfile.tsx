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

import DotDensityRenderer from "@arcgis/core/renderers/DotDensityRenderer";
import SimpleRenderer from "@arcgis/core/renderers/SimpleRenderer";
import FeatureEffect from "@arcgis/core/layers/support/FeatureEffect";
import SimpleFillSymbol from "@arcgis/core/symbols/SimpleFillSymbol";
import {
  type Dispatch,
  type SetStateAction,
  useEffect,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { DatasetMapLegend } from "./DatasetMapLegend";
import type { DatasetMapProfile, DatasetMapSlotProps } from "./profiles";
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
    initialPresentation: {
      effect: "drop-shadow(3px, 3px, 8px) bloom(0.15, .25px, .1)",
      renderer: createDotDensityRenderer(10),
    },
    mapSlotComponent: function CensusMapControls({
      clusterEnabled,
      headerActionsElement,
      onPresentationChange,
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
        onPresentationChange({
          renderer: demographicsEnabled
            ? createDotDensityRenderer(dotValue)
            : createBoundaryRenderer(),
          featureEffect: createPopulationFeatureEffect(
            demographicsEnabled,
            populationThreshold,
          ),
        });
      }, [
        demographicsEnabled,
        dotValue,
        onPresentationChange,
        populationThreshold,
      ]);

      return (
        <>
          <DatasetMapLegend
            clusterEnabled={clusterEnabled}
            hidden={!demographicsEnabled}
          />
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

function createBoundaryRenderer(): SimpleRenderer {
  return new SimpleRenderer({
    symbol: new SimpleFillSymbol({
      color: "black",
      outline: {
        color: [255, 255, 255, 0.4],
        width: "1px",
      },
    }),
  });
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

function createPopulationFeatureEffect(
  demographicsEnabled: boolean,
  populationThreshold: number,
): FeatureEffect | null {
  return demographicsEnabled && populationThreshold > 0
    ? new FeatureEffect({
        filter: { where: `P005001 > ${populationThreshold}` },
        includedEffect: "drop-shadow(2px, 2px, 12px)",
        excludedEffect: "grayscale(80%)",
      })
    : null;
}
