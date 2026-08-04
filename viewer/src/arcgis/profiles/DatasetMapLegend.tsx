interface DatasetMapLegendProps {
  clusterEnabled: boolean;
  hidden?: boolean;
}

export const DatasetMapLegend = ({
  clusterEnabled,
  hidden = false,
}: DatasetMapLegendProps) => (
  <arcgis-legend hidden={clusterEnabled || hidden} slot="bottom-left" />
);
