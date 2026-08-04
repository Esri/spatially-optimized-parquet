import type ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import type { ParquetLayerProperties } from "@arcgis/core/layers/ParquetLayer";
import type { ComponentType } from "react";
import { AlaskaProfile } from "./AlaskaProfile";
import { createCensusProfile } from "./CensusProfile";
import type { DatasetId } from "../../common/dataset/datasets";
import { createFranceProfile } from "./FranceProfile";
import { JapanProfile } from "./JapanProfile";

export interface DatasetEffectLayer {
  effect: string | null;
}

export interface DatasetLayerPresentation {
  featureEffect: ParquetLayer["featureEffect"];
  renderer: ParquetLayer["renderer"];
}

export interface DatasetMapSlotProps {
  clusterEnabled: boolean;
  headerActionsElement: HTMLElement | null;
  onPresentationChange(
    presentation: Partial<DatasetLayerPresentation>,
  ): void;
}

export type DatasetProfileCleanup = () => void;

export interface DatasetMapProfile {
  layerProperties?: Pick<ParquetLayerProperties, "popupTemplate" | "renderer">;
  mapSlotComponent?: ComponentType<DatasetMapSlotProps>;
  configureLayer?: (layer: DatasetEffectLayer) => DatasetProfileCleanup | undefined;
}

export const defaultDatasetMapProfile: DatasetMapProfile = {};

const datasetMapProfileRegistry: Partial<Record<DatasetId, DatasetMapProfile>> = {
  "alaska-3d-hydrography": AlaskaProfile,
  "census-blocks": createCensusProfile(),
  "building-footprints-japan": JapanProfile,
  "national-building-database-france": createFranceProfile(),
};


export function resolveDatasetMapProfile(
  datasetId: DatasetId | undefined,
): DatasetMapProfile {
  return datasetId
    ? datasetMapProfileRegistry[datasetId] ?? defaultDatasetMapProfile
    : defaultDatasetMapProfile;
}