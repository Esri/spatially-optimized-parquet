import type { ParquetLayerProperties } from "@arcgis/core/layers/ParquetLayer";
import type { ComponentType } from "react";
import type {
  LayerPresentationChange,
  LayerPresentationProperties,
} from "../layerPresentation";
import { AlaskaProfile } from "./AlaskaProfile";
import { createCensusProfile } from "./CensusProfile";
import type { DatasetId } from "../../common/dataset/datasets";
import { createFranceProfile } from "./FranceProfile";
import { JapanProfile } from "./JapanProfile";

export interface DatasetMapSlotProps {
  clusterEnabled: boolean;
  headerActionsElement: HTMLElement | null;
  onPresentationChange(
    presentation: LayerPresentationChange,
  ): void;
}

export interface DatasetMapProfile {
  initialPresentation?: Partial<LayerPresentationProperties>;
  layerProperties?: Pick<ParquetLayerProperties, "popupTemplate">;
  mapSlotComponent?: ComponentType<DatasetMapSlotProps>;
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