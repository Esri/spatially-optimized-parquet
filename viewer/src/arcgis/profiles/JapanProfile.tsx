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

import type { DatasetMapProfile } from "./profiles";

export const JapanProfile = {
  layerProperties: {
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
  initialPresentation: {
    effect: "drop-shadow(3px, 3px, 8px) bloom(0.15, .25px, .1)",
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
  },
} satisfies DatasetMapProfile;
