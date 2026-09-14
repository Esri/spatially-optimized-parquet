// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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