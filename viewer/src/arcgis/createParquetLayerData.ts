import ParquetFilesData from "@arcgis/core/layers/support/ParquetFilesData";
import ParquetPortalItemData from "@arcgis/core/layers/support/ParquetPortalItemData";

import type { Dataset } from "../common/dataset/datasets";

export type ArcgisParquetLayerData =
  | ParquetFilesData
  | ParquetPortalItemData;

export function createParquetLayerData(
  dataset: Dataset,
): ArcgisParquetLayerData {
  if (dataset.parquet.type === "url") {
    return new ParquetFilesData({ urls: [dataset.parquet.url] });
  }

  return new ParquetPortalItemData({
    portalItem: {
      id: dataset.parquet.itemId,
      portal: { url: dataset.parquet.portalUrl },
    },
  });
}
