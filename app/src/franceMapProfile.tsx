import { useEffect, useRef, useState } from "react";
import type {
  DatasetMapProfile,
  DatasetMapSlotProps,
} from "./datasetMapProfiles";

const minimumConstructionYear = 1800;
const maximumConstructionYear = 2024;
const initialConstructionYear = maximumConstructionYear;
const animationStepMilliseconds = 50;

export const franceMapProfile = {
  layerProperties: {
    renderer: createConstructionYearRenderer(initialConstructionYear),
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
  mapSlotComponent: function FranceMapControls({ layer }: DatasetMapSlotProps) {
    const [constructionYear, setConstructionYear] = useState(
      initialConstructionYear,
    );
    const [playing, setPlaying] = useState(false);
    const animationFrameRef = useRef<number | null>(null);

    useEffect(() => {
      if (!layer) {
        return;
      }

      layer.renderer = createConstructionYearRenderer(constructionYear);
    }, [constructionYear, layer]);

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
        <arcgis-legend slot="bottom-left" />
        <div className="map-renderer-controls" slot="bottom-right">
          <div className="map-renderer-controls-heading">
            Construction through{" "}
            <strong aria-live="polite">{constructionYear}</strong>
          </div>
          <calcite-slider
            label="Construction year"
            labelHandles
            max={maximumConstructionYear}
            min={minimumConstructionYear}
            oncalciteSliderInput={(event: Event) => {
              setPlaying(false);
              setConstructionYear(
                Number((event.currentTarget as HTMLCalciteSliderElement).value),
              );
            }}
            value={constructionYear}
          />
          <calcite-button
            appearance="solid"
            iconStart={playing ? "pause" : "play"}
            kind="brand"
            label={playing ? "Pause construction animation" : "Play construction animation"}
            onClick={() => setPlaying((currentPlaying) => !currentPlaying)}
          >
            {playing ? "Pause" : "Play"}
          </calcite-button>
        </div>
      </>
    );
  },
} satisfies DatasetMapProfile;

function createConstructionYearRenderer(year: number) {
  return {
    type: "simple" as const,
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
  };
}
