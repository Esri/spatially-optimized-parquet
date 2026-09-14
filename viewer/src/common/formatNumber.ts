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

export function formatInteger(value: number): string {
  return new Intl.NumberFormat().format(value);
}

export function formatPercent(
  value: number,
  maximumFractionDigits = 0,
): string {
  return `${new Intl.NumberFormat(undefined, {
    maximumFractionDigits,
  }).format(value)}%`;
}

export function formatRatio(
  numerator: number,
  denominator: number,
  fractionDigits: number,
  suffix: string,
): string | null {
  return denominator > 0
    ? `${(numerator / denominator).toFixed(fractionDigits)}${suffix}`
    : null;
}
