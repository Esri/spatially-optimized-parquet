# Spatially Optimized Parquet

Spatially Optimized Parquet (SOP) defines an additional layer of cluster-based spatial optimization on top of [Apache Parquet](https://parquet.apache.org/), focused on client-oriented streaming of Parquet files for display. SOP allows a single copy of the data to be used for both analytical queries and display streaming, without the need for a separate set of pregenerated tiles. Because SOP is untiled, it maintains feature integrity, avoiding the issues that tile-clipping can introduce. Tile-clipping can negatively impact both client-side analytics and geometric, expression-based visualization.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [[RFC2119](https://tools.ietf.org/html/rfc2119)].

## Full Resolution Geometry

While SOP-aware clients can function with just SOP, it's recommended to also include [GeoParquet](https://geoparquet.org/releases/v1.1.0/) for defining the full resolution geometry. When present, these full resolution geometries must be in the same spatial reference as the display optimization. A client may use these full resolution geometries for client-side analytics.

## Display Optimization

Spatially Optimized Parquet uses spatial clustering to colocate nearby geographic features. Clients can leverage statistic-based predicate skipping through the use of a [Page Index](https://parquet.apache.org/docs/file-format/pageindex/), to stream in the subset of the data needed for a given extent. For complex geometries, SOP also incudes several pregenerated multiscale LOD columns. These multiscale geometries reduce the amount of geometeric detail required at a given scale. Only two types of clustering are currently supported: `xz` for complex geometries, and `z` for points. Future versions of this specification may include additional clustering types, especially to support spatiotemporal datasets.

Metadata about how the file has been organized must be present in one of two places:

1. As a `geodisplay` key-value pair in the root metadata of the Parquet file.
2. In Spark-authored files, as Spark group-level metadata on `geodisplay` column containing the optimization structures.

### DisplayMetadata

`DisplayMetadata` defines the shared interface used by both clustering types. Unless otherwise noted, all fields are required, and at least one of `wkid` or `wkt` must be defined.

```ts
interface DisplayMetadata {
  type: "xz" | "z";
  version: string;
  writer?: Writer;
  fullExtent: Extent;
  wkid?: number;
  wkt?: string;
  geometryType: "point" | "multipoint" | "polygon" | "polyline";
  hasZ: boolean;
  hasM: boolean;
}
```

where:

| Field name | Description |
| --- | --- |
| `type` | Must be `"xz"` or `"z"`. |
| `version` | Current version of this specification. |
| `writer` | Optional writer metadata. |
| `fullExtent` | Full geographical extent of the geometries in the clustering spatial reference. |
| `wkid` | Optional EPSG or Esri latest WKID for the spatial reference used by the clustering. Currently only `4326` or `3857` are supported. |
| `wkt` | Optional spatial reference WKT. Used when `wkid` is undefined. |
| `geometryType` | Geometry type for all geometries in the clustering. One of `"point"`, `"multipoint"`, `"polygon"`, or `"polyline"`. |
| `hasZ` | Whether geometries in the clustering contain Z values. Currently this must be `false`. |
| `hasM` | Whether geometries in the clustering contain M values. Currently this must be `false`. |

#### Writer

The `Writer` metadata identifies the authoring application that produced the display optimization. It is recommended that writers include this information.

```ts
interface Writer {
  name: string;
  version: string;
}
```

where:

| Field name | Description |
| --- | --- |
| `name` | Name of the writer. |
| `version` | Version of the writer. |

#### Extent

An `Extent` defines the bounds of a set of the geometries in the spatial reference used by the display optimization. As will be described later, the `fullExtent` specified in the `DisplayMetadata` will be used in the encoding of features' XZ-codes.

```ts
interface Extent {
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}
```

where:

| Field name | Description |
| --- | --- |
| `xmin` | Minimum x-coordinate. |
| `ymin` | Minimum y-coordinate. |
| `xmax` | Maximum x-coordinate. |
| `ymax` | Maximum y-coordinate. |


## XZ Clustering

XZ-clustering is used for complex geometries. It encodes feature extents into a series of XZ-codes. After sorting on these codes, nearby features are colocated. And then additional multiple multiscale geometry columns enable progressive display at different scales.

### XZ Metadata

```ts
interface XZMetadata extends DisplayMetadata {
  type: "xz";
  maxLevel: number;
  field: string;
  code: string;
  levels: Array<MultiscaleLevel>,
  encoding: string;
}
```

where:

| Field name  | Description |
| --- | --- |
| `type`  | Must be `"xz"`. |
| `maxLevel` | Maximum XZ level used when generating XZ-codes. Currently this must be `20`. |
| `field` | Name of the group field containing the XZ cluster key and multiscale columns. |
| `code`  | Name of the column containing the XZ-code for the feature. Relative to `field`. |
| `levels` | Multiscale levels. There must be at least one multiscale column. Relative to `field`. |
| `encoding` | Encoding format for all geometries in the index. Currently only `"esriPBF"` is supported. |

#### Field Grouping

The metadata specifies the group `field` name that contains all additional columns generated by the display optimization. All other column names in the XZ metadata refer to columns that appear directly within this group field.

For a group `field` named `sop`, a `code` column named `xz-code` must be present at `sop.xz-code`, and a multiscale level named `multiscale-0` similarly must be present at `sop.multiscale-0`.

#### Multiscale level

Each `MultiscaleLevel` stores geometry quantized for a given map level. When rendering, clients choose the level that matches the current map scale, and may fallback to the full resolution geometry when zooming in past the last generated level.

```ts
interface MultiscaleLevel {
  column: string;
  level: number;
  resolution: number;
  scale: number;
  transform: QuantizationTransform;
}
```

where:

| Field name | Description |
| --- | --- |
| `column` | Name of the column where geometries for the multiscale level are stored. |
| `level` | Level associated with the multiscale column. This must be a number from `0` to `20`. |
| `resolution` | Resolution of the level. |
| `scale` | Scale of the level. |
| `transform` | `QuantizationTransform` needed to unquantize geometries in the multiscale column. |

#### Multiscale Transform

The `QuantizationTransform` carries the scale and translation values required to unquantize the included multiscale geometries. Both scale and translate are 4-width tuples for x, y, z, and m respectively. Tuples must always contain 4 values. When Z or M values are not present, it is recommended to use 1 and 0 for the empty scale and translate values respectively.

```ts
interface QuantizationTransform {
  scale: [number, number, number, number];
  translate: [number, number, number, number];
}
```

where:

| Field name | Description |
| --- | --- |
| `scale` | Quantization scale values. |
| `translate` | Quantization translation values. |

#### Multiscale Requirements

Multiscale geometry columns must be non-null. Quantization may degenerate geometries, but a degenerated geometry must include at least one coordinate with a single length of `1`. This allows clients to still symbolize these features. When quantized a source geometry  that is null to begin with, write an empty PBF geometry with an empty or missing `coords` and `lengths` array message. For polygons, exterior rings are generally assumed to be clockwise and interior rings, counterclockwise. However, quantization may degenerate polygons and violate that winding order, so clients must handle unexpected winding order for degenerated polygons.

#### Example

The following example shows XZ `geodisplay` metadata:

```json
{
  "geodisplay": {
    "version": "0.1",
    "type": "xz",
    "field": "sop",
    "wkid": 4326,
    "code": "xz-code",
    "geometryType": "polygon",
    "maxLevel": 20,
    "hasZ": false,
    "hasM": false,
    "fullExtent": {
      "xmin": -180,
      "ymin": -90,
      "xmax": 180,
      "ymax": 90
    },
    "encoding": "esriPBF",
    "levels": [
      {
        "column": "multiscale-0",
        "level": 0,
        "resolution": 0.703125,
        "scale": 295828763.7958547,
        "transform": {
          "scale": [0.703125, 0.703125, 1, 1],
          "translate": [0, 0, 0, 0]
        }
      },
      {
        "column": "multiscale-2",
        "level": 2,
        "resolution": 0.17578125,
        "scale": 73957190.94896367,
        "transform": {
          "scale": [0.17578125, 0.17578125, 1, 1],
          "translate": [0, 0, 0, 0]
        }
      }
    ]
  }
}
```

### Generating XZ-codes

XZ-code generation first projects data into the desired spatial reference. After computing the global extent of the projected data, per-feature XZ-codes are generated based on this global extent. Finally the dataset is sorted on the computet XZ-codes:

1. Project features to the target spatial reference of the index.
2. Compute the `fullExtent` of the projected features.
3. Use `fullExtent`, each feature extent, and the `maxLevel` value of `20` to calculate the XZ-code for each feature.
4. Sort features by XZ-code.

After clustering, clients can compute the XZ-covering of a given spatial query, and use Parquet page statistics to skip unrelated feature ranges.

#### Encoding Extents

To encode the extent of each feature as an XZ-code, first select a level based on the size of the feature's extent. Compare the width and height to the dataset's `fullExtent` width and height, and take the smaller of the two ratios. Convert that to a power-of-two level, clamping to the `maxLevel` in the `XZMetadata`.

With the level identified, divide the `fullExtent` into a regular grid at that level, and determine the cell covering of the feature's extent. For XZ-based querying, clients look at a target cell and it's immediate left and lower neighbors. That means that if the feature's extent covers more than two cells in either direction, we need to move it up one level to ensure that it fits within the enlarged two-cell region required by XZ.

Finally, take the lower-left corner of the feature's extent, and encode it at the selected level. Start with the `fullExtent`, and repeatedly split into four quadrants. At each level, select the quadrant containing the lower-left corner of the feature's extent and add that quadrant's sequence to the XZ code.

For more information, look at the algorithm as defined in the [original paper](http://dx.doi.org/10.1007/3-540-48482-5_7).

### Generating Multiscale Columns

Multiscale columns store quantized, delta-encoded geometries at powers-of-two levels. Each column snaps features to one pixel at the target level, with writers removing collinear vertices:
1. Project features into the target spatial reference of the index.
2. Generate powers-of-two multiscale levels from a starting scale of `295828763.79585470937713011037`.
3. Optionally reduce the number of generated multiscale levels to no fewer than one.
4. Create `QuantizedGeometry` with the quantization and delta-encoding algorithm.
5. PBF-encode `QuantizedGeometry` according to the Esri FeatureCollection PBF encoding. Only `lengths` and `coords` are required.

#### Select Multiscale Levels

Select multiscale levels by first picking a starting scale and resolution needed to snap vertices to a single pixel. Then iterate by powers of two. For example, the levels for WGS84 can be computed as below:

```rust
fn generate_levels_wgs84(max_levels: u32) -> Vec<MultiscaleLevel> {
  let mut levels = [];
  let mut resolution = 0.703125;
  let mut scale = 295828763.79585470937713011037;
  let mut level = 0;

  while (level <= max_levels) {
    levels.push(Level {
      column: `multiscale-${level}`,
      level,
      resolution,
      scale,
      transform: {
        scale: [resolution, resolution, 1., 1.],
        translate: [0., 0., 0., 0.]
      }
    });

    level += 1;
    scale /= 2.;
    resolution /= 2.;
  }

  levels
}
```

It is not required to generate every level. Clients should pick the closest, more-detailed level at any given scale.

#### 2D Quantization

For each feature, take it's geometry and for each multiscale column, quantize and delta-encode its rings using the `QuantizationTransform` associate with each level. Merge collinear vertices during quantization:

```rust
fn quantize_rings(geometry: &Geometry, transform: &QuantizationTransform) -> Geometry {
  let mut out = Geometry::new();

  for ring in &geometry.rings {
    let mut vertices = ring.vertices();
    let mut prev_delta = Vextex { x: 0, y: 0 };
    let mut prev_vertex = quantize(vertices.next(), transform);
    out.push(prev_vertex);

    for vertex in vertices {
      let vertex = quantize(vertex, transform);
      if !vertex.equals(prev_vertex) {
        let delta = vertex - prev_vertex;
        if is_collinear(prev_delta, delta) {
          *out.last() += delta;
        } else {
          out.push(vertex);
          prev_delta = delta;
        }
      }
    }

    out.close_ring();
  }

  out
}
```

Use `quantize` for individual verticies:

```rust
fn quantize(vertex: &Vertex, transform: &QuantizationTransform) -> Vertex {
  let x = f64::round((vertex.x - transform.translate[0]) / transform.scale[0]) as i64;
  let y = f64::round((vextex.y - transform.translate[1]) / transform.scale[1]) as i64;

  Vertex { x, y }
}
```



#### 3D Quantization

Not currently supported.


#### Encoding

Quantized, delta-encoded multiscale geometries are written to Parquet BYTE_ARRAY columns, after being encoded as a [ProtocolBuffers](https://protobuf.dev/) message with the following schema:

```proto
message Geometry {
  repeated uint32 lengths = 2 [packed = true];
  repeated sint64 coords = 3 [packed = true];
}
```

This is a flattened, delta-encoded geometry representation, where each length in the lengths array points to the start of a ring or path.

For example, take the following polygon with a ring and a hole:

```
Polygon {
  rings: [
    [[256, 256], [258, 256], [258, 258], [256, 258], [256, 256]], // outer ring
    [[56, 56], [58, 56], [58, 58], [56, 58], [56, 56]] // hole
}
```

It becomes this corresponding flattened geometry:

```
Geometry: {
  lengths: [5, 5]
  coords: [256, 256, 2, 0, 0, 2, -2, 0, 0, -2, 56, 56, 2, 0, 0, 2, -2, 0, 0, -2]
}
```


## Z Clustering

For point features, simple Z-clustering is used. XZ-codes are unnecessary for points because geometries do not carry a spatial extent, and point display does not require multiscale geometry columns.

Unlike with XZ, Z-clustering does not require a group field. An implementation may point directly to original `x` and `y` columns, provided those columns already use the desired display spatial reference.

### Z Metadata

```ts
interface ZMetadata extends DisplayMetadata {
  type: "z";
  geometryType: "point";
  code: string;
  xColumn: string;
  yColumn: string;
  zColumn?: string;
  mColumn?: string;
  coordinatePrecision: number;
}
```

where:

| Field name | Description |
| --- | --- |
| `type` | Must be `"z"`. |
| `geometryType` | Must be `"point"`. |
| `code` | Name of the column containing Z-codes for points within the clustering. |
| `xColumn` | Name of the non-nullable column containing point x-values. A null or invalid geometry should include `NaN`. |
| `yColumn` | Name of the column containing point y-values. A null or invalid geometry should include `NaN`. |
| `zColumn` | Optional column containing point z-values. A null or invalid geometry should include `NaN`. |
| `mColumn` | Optional column containing point m-values. A null or invalid geometry should include `NaN`. |
| `coordinatePrecision` | Number of bits of precision used for each coordinate when generating the Z-code. |

#### Example

The following example shows `geodisplay` metadata for a file containing a single Z-index.

```json
{
  "geodisplay": {
    "version": "0.1",
    "type": "z",
    "code": "z-code",
    "geometryType": "point",
    "hasZ": false,
    "hasM": false,
    "fullExtent": {
      "xmin": -180,
      "ymin": -90,
      "xmax": 180,
      "ymax": 90
    },
    "wkid": 4326,
    "xColumn": "x-values",
    "yColumn": "y-values",
    "coordinatePrecision": 16
  }
}
```

### Generating Z-codes

Z-code generation normalizes projected point coordinates into the index extent and interleaves their quantized coordinate bits. The resulting code gives point features a spatial ordering that Parquet readers can query with page statistics.

1. Project features into the spatial reference of the index.
2. Calculate the `fullExtent` of the data.
3. Normalize each coordinate into the range `0` through `1` using `fullExtent`.
4. Multiply each normalized coordinate by the maximum unsigned integer represented by `coordinatePrecision`.
5. Compute the Z-code by swizzling the quantized x and y values for each point.

```rust

 fn generate_morton_code(quantized_x: u64, quantized_y: u64, coordinate_precision: u32) -> u64 {
   let mut code = 0u64;

   for bit_index in 0..coordinate_precision {
     let x_bit = (quantized_x >> bit_index) & 1;
     let y_bit = (quantized_y >> bit_index) & 1;

     code |= x_bit << (2 * bit_index);
     code |= y_bit << (2 * bit_index + 1);
   }

   code
 }
```
