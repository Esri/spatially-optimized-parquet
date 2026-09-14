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

import { DatasetMapLegend } from "./DatasetMapLegend";
import type { DatasetMapProfile, DatasetMapSlotProps } from "./profiles";

export const AlaskaProfile = {
  layerProperties: {
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
  initialPresentation: {
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
  },
  mapSlotComponent: function AlaskaMapLegend({
    clusterEnabled,
  }: DatasetMapSlotProps) {
    return <DatasetMapLegend clusterEnabled={clusterEnabled} />;
  },
} satisfies DatasetMapProfile;
