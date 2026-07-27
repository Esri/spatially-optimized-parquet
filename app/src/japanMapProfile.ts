import type { DatasetMapProfile } from "./datasetMapProfiles";

export const japanMapProfile = {
  layerProperties: {
    renderer: {
      type: "simple",
      symbol: {
        type: "simple-fill",
        color: "black",
        outline: {
          color: [255, 255, 255, 0.6],
          width: 1,
        }
      },
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
