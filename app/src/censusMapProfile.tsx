import DotDensityRenderer from "@arcgis/core/renderers/DotDensityRenderer";
import { useEffect, useState } from "react";
import type {
  DatasetMapProfile,
  DatasetMapSlotProps,
} from "./datasetMapProfiles";

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

export const censusMapProfile = {
  layerProperties: {
    renderer: createBoundaryRenderer(),
  },
  mapSlotComponent: function CensusMapControls({ layer }: DatasetMapSlotProps) {
    const [demographicsEnabled, setDemographicsEnabled] = useState(false);
    const [dotValue, setDotValue] = useState(10);
    const [populationThreshold, setPopulationThreshold] = useState(0);

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
        <div className="census-renderer-controls" slot="bottom-right">
          <div className="census-renderer-controls-heading">Census blocks</div>
          <calcite-button
            appearance="solid"
            kind="brand"
            label={demographicsEnabled ? "Show boundaries" : "Show demographics"}
            onClick={() => {
              setDemographicsEnabled((enabled) => !enabled);
              setPopulationThreshold(0);
            }}
          >
            {demographicsEnabled ? "Show boundaries" : "Show demographics"}
          </calcite-button>
          {demographicsEnabled ? (
            <>
              <calcite-label>
                Dots per person
                <calcite-slider
                  label="Dots per person"
                  labelHandles
                  max={200}
                  min={1}
                  oncalciteSliderInput={(event: Event) =>
                    setDotValue(
                      Number(
                        (event.currentTarget as HTMLCalciteSliderElement).value,
                      ),
                    )
                  }
                  value={dotValue}
                />
              </calcite-label>
              <calcite-label>
                Emphasize by population count
                <calcite-slider
                  label="Emphasize by population count"
                  labelHandles
                  max={400}
                  min={0}
                  oncalciteSliderInput={(event: Event) =>
                    setPopulationThreshold(
                      Number(
                        (event.currentTarget as HTMLCalciteSliderElement).value,
                      ),
                    )
                  }
                  value={populationThreshold}
                />
              </calcite-label>
            </>
          ) : null}
        </div>
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
} satisfies DatasetMapProfile;

function createBoundaryRenderer() {
  return {
    type: "simple" as const,
    symbol: {
      type: "simple-fill" as const,
      color: "black",
      outline: {
        color: [255, 255, 255, 0.3],
        width: 1,
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
    attributes: [
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
    ],
  });
}
