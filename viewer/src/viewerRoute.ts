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
