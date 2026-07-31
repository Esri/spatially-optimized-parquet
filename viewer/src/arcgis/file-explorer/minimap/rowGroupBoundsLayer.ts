import FeatureLayer from "@arcgis/core/layers/FeatureLayer";
import Graphic from "@arcgis/core/Graphic";

import type { RowGroupBounds } from "../../../parquet/rowGroupBounds";

export function createRowGroupBoundsLayer(
  bounds: readonly RowGroupBounds[],
  labelsVisible = true,
): FeatureLayer {
  return new FeatureLayer({
    title: "Parquet row group bounds",
    geometryType: "polygon",
    spatialReference: { wkid: 4326 },
    objectIdField: "OBJECTID",
    legendEnabled: false,
    fields: [
      { name: "OBJECTID", type: "oid" },
      { name: "ROW_GROUP", type: "integer" },
      { name: "COLOR_CLASS", type: "integer" },
    ],
    source: bounds.map(
      ({ rowGroupIndex, xmin, ymin, xmax, ymax }) =>
        new Graphic({
          attributes: {
            OBJECTID: rowGroupIndex + 1,
            ROW_GROUP: rowGroupIndex,
            COLOR_CLASS: rowGroupIndex % 3,
          },
          geometry: {
            type: "polygon",
            rings: [[
              [xmin, ymin],
              [xmax, ymin],
              [xmax, ymax],
              [xmin, ymax],
              [xmin, ymin],
            ]],
            spatialReference: { wkid: 4326 },
          },
        }),
    ),
    labelingInfo: labelsVisible
      ? [
          createDebugLabelClass(0, "#ef3573"),
          createDebugLabelClass(1, "#5bff94"),
          createDebugLabelClass(2, "#3ec5ff"),
        ]
      : [],
    renderer: {
      type: "class-breaks",
      field: "COLOR_CLASS",
      classBreakInfos: [
        { minValue: -0.5, maxValue: 0.5, symbol: createDebugBoundsSymbol("#ef3573") },
        { minValue: 0.5, maxValue: 1.5, symbol: createDebugBoundsSymbol("#5bff94") },
        { minValue: 1.5, maxValue: 2.5, symbol: createDebugBoundsSymbol("#3ec5ff") },
      ],
    },
  });
}

function createDebugBoundsSymbol(outlineColor: string) {
  return {
    type: "simple-fill" as const,
    color: [0, 0, 0, 0],
    outline: { color: outlineColor, width: 1 },
  };
}

function createDebugLabelClass(colorClass: number, textColor: string) {
  return {
    where: `COLOR_CLASS = ${colorClass}`,
    labelExpressionInfo: { expression: "'RG ' + Text($feature.ROW_GROUP)" },
    labelPlacement: "always-horizontal" as const,
    symbol: {
      type: "text" as const,
      color: textColor,
      haloColor: [0, 0, 0, 0.8],
      haloSize: 2,
      font: { family: "Arial", size: 14, weight: "bold" as const },
    },
  };
}
