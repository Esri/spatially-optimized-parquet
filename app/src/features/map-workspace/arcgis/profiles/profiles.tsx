import type ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import type { ParquetLayerProperties } from "@arcgis/core/layers/ParquetLayer";
import type { ComponentType } from "react";
import { AlaskaProfile } from "./AlaskaProfile";
import { CensusProfile } from "./CensusProfile";
import type { DatasetId } from "../../datasets";
import { FranceProfile } from "./FranceProfile";
import { JapanProfile } from "./JapanProfile";

export interface DatasetEffectLayer {
  effect: string | null;
}

export interface DatasetMapSlotProps {
  headerActionsElement: HTMLElement | null;
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
  "alaska-3d-hydrography": AlaskaProfile,
  "census-blocks": CensusProfile,
  "building-footprints-japan": JapanProfile,
  "national-building-database-france": FranceProfile,
};

export function resolveDatasetMapProfile(datasetId: DatasetId): DatasetMapProfile {
  return datasetMapProfileRegistry[datasetId] ?? defaultDatasetMapProfile;
}
