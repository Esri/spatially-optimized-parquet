import type { DatasetMapProfile } from "./datasetMapProfiles";

export const alaskaMapProfile = {
  layerProperties: {
    renderer: {
      type: "simple",
      symbol: {
        type: "simple-line",
        color: [100, 116, 139, 0.25],
        width: 0.5,
      },
      visualVariables: [
        {
          type: "color",
          field: "lengthkm",
          stops: [
            {
              value: 0.13,
              color: [56, 189, 248, 0.5],
              label: "0.13 km",
            },
            {
              value: 0.3,
              color: [34, 211, 238, 0.6],
              label: "0.30 km",
            },
            {
              value: 0.64,
              color: [103, 232, 249, 0.7],
              label: "0.64 km",
            },
            {
              value: 1.86,
              color: [165, 243, 252, 0.8],
              label: "1.86 km",
            },
            {
              value: 3.73,
              color: [224, 251, 252, 0.9],
              label: "3.73 km or longer",
            },
          ],
        },
      ],
    },
    popupTemplate: {
      title: "{featuretypelabel}",
      content: [
        {
          type: "fields",
          fieldInfos: [
            {
              fieldName: "lengthkm",
              label: "Length (km)",
              format: { digitSeparator: true, places: 2 },
            },
            {
              fieldName: "featuredate",
              label: "Feature date",
            },
          ],
        },
      ],
    },
  },
  mountMapComponents({ mapElement }) {
    const legendElement = document.createElement("arcgis-legend");
    legendElement.setAttribute("slot", "bottom-left");
    mapElement.append(legendElement);

    return () => {
      legendElement.remove();
    };
  },
} satisfies DatasetMapProfile;
