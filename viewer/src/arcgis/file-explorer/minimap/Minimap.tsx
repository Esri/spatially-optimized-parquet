import Basemap from "@arcgis/core/Basemap";
import * as reactiveUtils from "@arcgis/core/core/reactiveUtils";
import Extent from "@arcgis/core/geometry/Extent";
import Graphic from "@arcgis/core/Graphic";
import GraphicsLayer from "@arcgis/core/layers/GraphicsLayer";
import VectorTileLayer from "@arcgis/core/layers/VectorTileLayer";
import SpatialReference from "@arcgis/core/geometry/SpatialReference";
import { memo, useEffect, useMemo, useRef, useState, type RefObject } from "react";

import type { Dataset } from "../../../common/dataset/datasets";
import { calculateXZFocusExtent } from "../../../common/xz_bounds/defaultExtent";
import type { ApproximateBound } from "../../../common/xz_bounds/approximateBounds";
import { createRowGroupBoundsLayer } from "./rowGroupBoundsLayer";
import styles from "./Minimap.module.css";

/**
 * Shows row-group bounds beside the main map and mirrors its current extent.
 * It owns the two-way view coordination so contributors can change overview behavior without coupling it to the main viewer.
 */
export const Minimap = memo(function Minimap({
  bounds,
  boundsApproximate,
  dataset,
  diagnosticsReady,
  fullExtent,
  mainMapElementRef,
}: {
  bounds: readonly ApproximateBound[] | null;
  boundsApproximate: boolean;
  dataset: Dataset;
  diagnosticsReady: boolean;
  fullExtent: Extent | null;
  mainMapElementRef: RefObject<HTMLArcgisMapElement | null>;
}) {
  const mapElementRef = useRef<HTMLArcgisMapElement>(null);
  const [overviewReady, setOverviewReady] = useState(false);
  const basemap = useMemo(
    () => createDatasetBasemap(dataset.basemap),
    [dataset.basemap],
  );
  const spatialReference = useMemo(
    () => createDatasetSpatialReference(dataset.spatialReference),
    [dataset.spatialReference],
  );

  useEffect(() => {
    const mapElement = mapElementRef.current;
    if (!mapElement) {
      return;
    }

    mapElement.center = dataset.center;
    mapElement.scale = dataset.scale;
    let disposed = false;
    let boundsLayer: ReturnType<typeof createRowGroupBoundsLayer> | undefined;
    let extentLayer: GraphicsLayer | undefined;
    let extentWatcher: { remove(): void } | undefined;
    let overviewClickHandle: { remove(): void } | undefined;
    let overviewDragHandle: { remove(): void } | undefined;

    const loadOverview = async () => {
      try {
        await mapElement.viewOnReady();
        const map = mapElement.map;
        if (!map) {
          throw new Error("Row group overview map is unavailable.");
        }
        if (bounds) {
          boundsLayer = createRowGroupBoundsLayer(bounds, false);
          map.add(boundsLayer);
          await boundsLayer.when();
        }
        const focusExtent = bounds
          ? calculateXZFocusExtent(bounds)?.extent
          : undefined;
        const overviewExtent = focusExtent
          ? new Extent({
              ...focusExtent,
              spatialReference: { wkid: 4326 },
            })
          : fullExtent;
        if (!disposed && overviewExtent) {
          await mapElement.view.goTo(overviewExtent, { animate: false });
        }
        if (!disposed) {
          setOverviewReady(true);
        }
        const mainMapElement = mainMapElementRef.current;
        if (!mainMapElement || disposed) {
          return;
        }
        await mainMapElement.viewOnReady();
        if (disposed) {
          return;
        }
        const recenterMainView = (x: number, y: number) => {
          const mapCenter = mapElement.view.toMap({ x, y });
          if (mapCenter) {
            mainMapElement.view.center = mapCenter;
          }
        };
        overviewClickHandle = mapElement.view.on("immediate-click", (event) => {
          if (event.button === 0) {
            recenterMainView(event.x, event.y);
          }
        });
        overviewDragHandle = mapElement.view.on("drag", (event) => {
          if (event.button === 0) {
            event.stopPropagation();
            recenterMainView(event.x, event.y);
          }
        });
        const extentGraphic = new Graphic({
          geometry: mainMapElement.view.extent.clone(),
          symbol: {
            type: "simple-fill",
            color: [255, 255, 255, 0.2],
            outline: { color: [255, 255, 255, 1], width: 1.5 },
          },
        });
        extentLayer = new GraphicsLayer({
          title: "Current view extent",
          graphics: [extentGraphic],
        });
        map.add(extentLayer);
        extentWatcher = reactiveUtils.watch(
          () => mainMapElement.view.extent,
          (extent) => {
            extentGraphic.geometry = extent.clone();
          },
        );
      } catch (error) {
        if (!disposed) {
          console.error("Failed to load row group overview map.", error);
        }
      }
    };
    void loadOverview();

    return () => {
      disposed = true;
      setOverviewReady(false);
      overviewClickHandle?.remove();
      overviewDragHandle?.remove();
      extentWatcher?.remove();
      if (extentLayer) {
        mapElement.map?.remove(extentLayer);
      }
      if (boundsLayer) {
        mapElement.map?.remove(boundsLayer);
      }
    };
  }, [
    bounds,
    dataset.center,
    dataset.scale,
    fullExtent,
    mainMapElementRef,
  ]);

  return (
    <div className={styles.rowGroupOverview}>
      <div className={styles.rowGroupOverviewMapFrame}>
        <arcgis-map
          ref={mapElementRef}
          aria-label="Parquet row group overview"
          className={overviewReady ? styles.ready : undefined}
          spatialReference={spatialReference}
          basemap={basemap}
          center={dataset.center}
          scale={dataset.scale}
        />
        {!overviewReady || !diagnosticsReady ? (
          <div className={styles.rowGroupOverviewPlaceholder}>
            Loading row groups…
          </div>
        ) : null}
        {overviewReady &&
        diagnosticsReady &&
        (boundsApproximate || !bounds) ? (
          <div
            aria-label={
              boundsApproximate
                ? "Why row group bounds are approximate"
                : "Why row group bounds are unavailable"
            }
            className={styles.rowGroupBoundsUnavailable}
            id="row-group-bounds-info"
            tabIndex={0}
          >
            <span>
              {boundsApproximate
                ? "Row group bounds approximated"
                : "Row group bounds not supported"}
            </span>
            <span className={styles.rowGroupBoundsInfo} aria-hidden="true">
              <calcite-icon icon="information" scale="s" />
            </span>
            <calcite-tooltip
              overlayPositioning="fixed"
              referenceElement="row-group-bounds-info"
            >
              {boundsApproximate
                ? "Bounds are approximated from XZ column statistics. Exact row group bounds require newly added native spatial types with geospatial statistics."
                : "Row group bounds require newly added native spatial types with geospatial statistics."}
            </calcite-tooltip>
          </div>
        ) : null}
      </div>
    </div>
  );
});

function createDatasetBasemap(basemapId?: string): Basemap | string {
  return basemapId
    ? new Basemap({
        baseLayers: [new VectorTileLayer({ portalItem: { id: basemapId } })],
      })
    : "dark-gray-vector";
}

function createDatasetSpatialReference(wkid?: number): SpatialReference {
  return new SpatialReference({ wkid: wkid ?? 3857 });
}
