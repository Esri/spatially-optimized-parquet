# MapLibre reference implementation

This example code shows how to consume SOP files with MapLibre. As the map moves, the browser takes the current extent, converts it into a series of XZ code ranges, and then use the XZ keys present in the file to find the relevant parts of the file. It reads the best geometry level for the current zoom, decodes the selected features, and sends them to MapLibre as GeoJSON.

The reader delegates XZ row-group and page skipping to Hyparquet's page-index predicate pushdown. It then uses the exact matching row IDs to read one selected LOD leaf. This small physical-leaf adapter is necessary because Hyparquet 1.27.1's public API does not support selecting one nested physical leaf. Its public projection operates on the top-level `geolod` field, which would otherwise download and decode every sibling LOD column.

This code should just be taken as a reference, a real implementation will likely want to tile the view, and to pass the data to the rendering engine without first converting to GeoJSON.
