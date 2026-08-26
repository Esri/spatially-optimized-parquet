import {
  createCustomUrlDataset,
  createPortalItemDataset,
  getPortalDomain,
  type Dataset,
  datasets,
} from "./datasets";

const datasetParameter = "dataset";
const customUrlParameter = "url";
const portalUrlParameter = "portal";
const portalItemIdParameter = "id";
const centerParameter = "center";
const scaleParameter = "scale";
const customDatasetRoute = "custom";
const portalDatasetRoute = "portal";

export interface DatasetRoute {
  dataset: Dataset;
  viewpoint: DatasetRouteViewpoint | null;
}

export interface DatasetRouteViewpoint {
  center: [number, number];
  scale: number;
}

export function resolveDatasetRoute(url: URL): DatasetRoute {
  const route = url.searchParams.get(datasetParameter);
  if (!route) {
    return { dataset: datasets[0], viewpoint: resolveViewpoint(url) };
  }

  const preset = datasets.find(({ id }) => id === route);
  if (preset) {
    return { dataset: preset, viewpoint: resolveViewpoint(url) };
  }

  if (route === customDatasetRoute) {
    return {
      dataset: createCustomUrlDataset(
        requireRouteParameter(url, customUrlParameter, "Custom dataset URL"),
      ),
      viewpoint: resolveViewpoint(url),
    };
  }

  if (route === portalDatasetRoute) {
    return {
      dataset: createPortalItemDataset(
        requireRouteParameter(url, portalUrlParameter, "Portal URL"),
        requireRouteParameter(url, portalItemIdParameter, "Portal item ID"),
      ),
      viewpoint: resolveViewpoint(url),
    };
  }

  throw new Error(`Unknown dataset route: ${route}`);
}

export function createDatasetRouteUrl(
  currentUrl: URL,
  dataset: Dataset,
): URL {
  const nextUrl = new URL(currentUrl);
  clearDatasetRoute(nextUrl);
  clearViewpointRoute(nextUrl);

  if (dataset.kind === "preset") {
    nextUrl.searchParams.set(datasetParameter, dataset.id);
    return nextUrl;
  }

  if (dataset.kind === "custom-url") {
    nextUrl.searchParams.set(datasetParameter, customDatasetRoute);
    nextUrl.searchParams.set(customUrlParameter, dataset.parquet.url);
    return nextUrl;
  }

  nextUrl.searchParams.set(datasetParameter, portalDatasetRoute);
  nextUrl.searchParams.set(
    portalUrlParameter,
    getPortalDomain(dataset.parquet.portalUrl),
  );
  nextUrl.searchParams.set(portalItemIdParameter, dataset.parquet.itemId);
  return nextUrl;
}

export function createViewpointRouteUrl(
  currentUrl: URL,
  viewpoint: DatasetRouteViewpoint,
): URL {
  const nextUrl = new URL(currentUrl);
  nextUrl.searchParams.set(
    centerParameter,
    viewpoint.center.map(formatCoordinate).join(","),
  );
  nextUrl.searchParams.set(scaleParameter, formatScale(viewpoint.scale));
  return nextUrl;
}

function clearDatasetRoute(url: URL): void {
  url.searchParams.delete(datasetParameter);
  url.searchParams.delete(customUrlParameter);
  url.searchParams.delete(portalUrlParameter);
  url.searchParams.delete(portalItemIdParameter);
}

function clearViewpointRoute(url: URL): void {
  url.searchParams.delete(centerParameter);
  url.searchParams.delete(scaleParameter);
}

function resolveViewpoint(url: URL): DatasetRouteViewpoint | null {
  const centerValue = url.searchParams.get(centerParameter);
  const scaleValue = url.searchParams.get(scaleParameter);
  if (!centerValue && !scaleValue) {
    return null;
  }
  if (!centerValue || !scaleValue) {
    throw new Error("Dataset center and scale must be provided together.");
  }

  const coordinates = centerValue.split(",").map(Number);
  const scale = Number(scaleValue);
  if (
    coordinates.length !== 2 ||
    !coordinates.every(Number.isFinite) ||
    coordinates[0] < -180 ||
    coordinates[0] > 180 ||
    coordinates[1] < -90 ||
    coordinates[1] > 90
  ) {
    throw new Error("Dataset center must contain valid longitude and latitude.");
  }
  if (!Number.isFinite(scale) || scale <= 0) {
    throw new Error("Dataset scale must be a positive number.");
  }

  return {
    center: [coordinates[0], coordinates[1]],
    scale,
  };
}

function formatCoordinate(value: number): string {
  return Number(value.toFixed(6)).toString();
}

function formatScale(value: number): string {
  return Number(value.toFixed(2)).toString();
}

function requireRouteParameter(
  url: URL,
  parameter: string,
  label: string,
): string {
  const value = url.searchParams.get(parameter);
  if (!value) {
    throw new Error(`${label} is required in the dataset URL.`);
  }
  return value;
}
