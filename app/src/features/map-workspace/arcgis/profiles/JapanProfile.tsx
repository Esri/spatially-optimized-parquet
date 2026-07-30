import type { DatasetMapProfile } from "./profiles";

export const JapanProfile = {
  layerProperties: {
    renderer: {
      type: "simple",
      symbol: {
        type: "simple-fill",
        color: "black",
        outline: {
          color: [255, 255, 255, 0.4],
          width: 1,
        },
      },
    },
    popupTemplate: {
      title: "Building",
      content: [
        {
          type: "fields",
          fieldInfos: [
            {
              fieldName: "is_underground",
              label: "Underground",
            },
            {
              fieldName: "has_parts",
              label: "Has building parts",
            },
          ],
        },
      ],
    },
  },
  configureLayer(layer) {
    const japanLayerEffect =
      "drop-shadow(3px, 3px, 8px) bloom(0.15, .25px, .1)";

    layer.effect = japanLayerEffect;

    return () => {
      if (layer.effect === japanLayerEffect) {
        layer.effect = null;
      }
    };
  },
} satisfies DatasetMapProfile;
