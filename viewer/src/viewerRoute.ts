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

export type ViewerType = "arcgis" | "maplibre";

const maplibreRouteSegment = "maplibre";

export function resolveViewerRoute(url: URL): ViewerType {
  return hasMaplibreRoute(url.pathname) ? "maplibre" : "arcgis";
}

export function createViewerRouteUrl(
  currentUrl: URL,
  viewer: ViewerType,
): URL {
  const nextUrl = new URL(currentUrl);
  const basePath = resolveBasePath(nextUrl.pathname);
  nextUrl.pathname = viewer === "maplibre"
    ? `${basePath}${maplibreRouteSegment}/`
    : basePath;
  return nextUrl;
}

function hasMaplibreRoute(pathname: string): boolean {
  return new RegExp(`/${maplibreRouteSegment}/?$`).test(pathname);
}

function resolveBasePath(pathname: string): string {
  const basePath = hasMaplibreRoute(pathname)
    ? pathname.replace(new RegExp(`${maplibreRouteSegment}/?$`), "")
    : pathname;

  return basePath.endsWith("/") ? basePath : `${basePath}/`;
}
