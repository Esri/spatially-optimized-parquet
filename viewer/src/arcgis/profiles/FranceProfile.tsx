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

import SimpleRenderer from "@arcgis/core/renderers/SimpleRenderer";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { DatasetMapLegend } from "./DatasetMapLegend";
import type { DatasetMapProfile, DatasetMapSlotProps } from "./profiles";
import styles from "./FranceProfile.module.css";

const minimumConstructionYear = 1800;
const maximumConstructionYear = 2024;
const initialConstructionYear = maximumConstructionYear;
const animationStepMilliseconds = 50;

export function createFranceProfile(): DatasetMapProfile {
  return {
    layerProperties: {
      popupTemplate: {
        title: "{libelle_commune_insee}",
        content: [
          {
            type: "fields",
            fieldInfos: [
              {
                fieldName: "ffo_bat_annee_construction",
                label: "Construction year",
                format: { digitSeparator: false, places: 0 },
              },
              {
                fieldName: "usage_principal_bdnb_open",
                label: "Primary use",
              },
              {
                fieldName: "s_geom_groupe",
                label: "Footprint area (m²)",
                format: { digitSeparator: true, places: 0 },
              },
              {
                fieldName: "bdtopo_bat_hauteur_mean",
                label: "Mean height (m)",
                format: { digitSeparator: true, places: 0 },
              },
              {
                fieldName: "ffo_bat_nb_niveau",
                label: "Levels",
                format: { digitSeparator: true, places: 0 },
              },
              {
                fieldName: "ffo_bat_nb_log",
                label: "Housing units",
                format: { digitSeparator: true, places: 0 },
              },
            ],
          },
        ],
      },
    },
    initialPresentation: {
      renderer: createConstructionYearRenderer(initialConstructionYear),
    },
    mapSlotComponent: function FranceMapControls({
      clusterEnabled,
      headerActionsElement,
      onPresentationChange,
    }: DatasetMapSlotProps) {
      const [constructionYear, setConstructionYear] = useState(
        initialConstructionYear,
      );
      const [playing, setPlaying] = useState(false);
      const animationFrameRef = useRef<number | null>(null);

      useEffect(() => {
        onPresentationChange({
          renderer: createConstructionYearRenderer(constructionYear),
        });
      }, [constructionYear, onPresentationChange]);

      useEffect(() => {
        if (!playing) {
          return;
        }

        let previousStepTime = performance.now();
        const animate = (currentTime: number) => {
          if (currentTime - previousStepTime >= animationStepMilliseconds) {
            previousStepTime = currentTime;
            setConstructionYear((year) =>
              year >= maximumConstructionYear
                ? minimumConstructionYear
                : year + 1,
            );
          }
          animationFrameRef.current = requestAnimationFrame(animate);
        };

        animationFrameRef.current = requestAnimationFrame(animate);

        return () => {
          if (animationFrameRef.current !== null) {
            cancelAnimationFrame(animationFrameRef.current);
            animationFrameRef.current = null;
          }
        };
      }, [playing]);

      return (
        <>
          <DatasetMapLegend clusterEnabled={clusterEnabled} />
          {headerActionsElement
            ? createPortal(
                <div className={styles.franceMapHeaderControls}>
                  <calcite-label layout="inline">
                    Year
                    <strong aria-live="polite">{constructionYear}</strong>
                  </calcite-label>
                  <calcite-slider
                    label="Construction year"
                    max={maximumConstructionYear}
                    min={minimumConstructionYear}
                    oncalciteSliderInput={(event: Event) => {
                      setPlaying(false);
                      setConstructionYear(
                        Number(
                          (event.currentTarget as HTMLCalciteSliderElement).value,
                        ),
                      );
                    }}
                    value={constructionYear}
                  />
                  <calcite-button
                    appearance="transparent"
                    iconStart={playing ? "pause" : "play"}
                    kind="neutral"
                    label={
                      playing
                        ? "Pause construction animation"
                        : "Play construction animation"
                    }
                    onClick={() =>
                      setPlaying((currentPlaying) => !currentPlaying)
                    }
                  />
                </div>,
                headerActionsElement,
              )
            : null}
        </>
      );
    },
  };
}

function createConstructionYearRenderer(year: number) {
  return new SimpleRenderer({
    symbol: {
      type: "simple-fill" as const,
      color: [100, 116, 139, 0.22],
      outline: null,
    },
    visualVariables: [
      {
        type: "opacity" as const,
        field: "ffo_bat_annee_construction",
        stops: [
          { value: year, opacity: 1 },
          { value: year + 1, opacity: 0 },
        ],
        legendOptions: { showLegend: false },
      },
      {
        type: "color" as const,
        field: "ffo_bat_annee_construction",
        legendOptions: { title: "Construction year" },
        stops: [
          {
            value: year - 50,
            color: "#401040",
            label: `Before ${year - 50}`,
          },
          {
            value: year - 10,
            color: "#ff00ff",
            label: `${year - 10}`,
          },
          {
            value: year,
            color: "#00ffff",
            label: `${year}`,
          },
        ],
      },
    ],
  });
}
