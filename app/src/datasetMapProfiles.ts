import type ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import type { ParquetLayerProperties } from "@arcgis/core/layers/ParquetLayer";
import type { ComponentType } from "react";
import { alaskaMapProfile } from "./alaskaMapProfile";
import { censusMapProfile } from "./censusMapProfile";
import type { DatasetId } from "./datasets";
import { franceMapProfile } from "./franceMapProfile";
import { japanMapProfile } from "./japanMapProfile";

export interface DatasetEffectLayer {
  effect: string | null;
}

export interface DatasetMapSlotProps {
  layer: ParquetLayer | null;
}

export interface DatasetMapComponentContext {
  mapElement: HTMLArcgisMapElement;
  layer: ParquetLayer;
}

export type DatasetProfileCleanup = () => void;

export interface DatasetMapProfile {
  layerProperties?: Pick<ParquetLayerProperties, "popupTemplate" | "renderer">;
  mapSlotComponent?: ComponentType<DatasetMapSlotProps>;
  configureLayer?: (layer: DatasetEffectLayer) => DatasetProfileCleanup | undefined;
  mountMapComponents?: (
    context: DatasetMapComponentContext,
  ) => DatasetProfileCleanup | undefined;
}

export const defaultDatasetMapProfile: DatasetMapProfile = {};

const datasetMapProfileRegistry: Partial<Record<DatasetId, DatasetMapProfile>> = {
  "alaska-3d-hydrography": alaskaMapProfile,
  "census-blocks": censusMapProfile,
  "building-footprints-japan": japanMapProfile,
  "national-building-database-france": franceMapProfile,
};

export function resolveDatasetMapProfile(datasetId: DatasetId): DatasetMapProfile {
  return datasetMapProfileRegistry[datasetId] ?? defaultDatasetMapProfile;
}
