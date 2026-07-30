import type {
  QuantizationTransform,
  XZDisplayMetadata,
} from "./datasetParquetMetadata";

interface PbfGeometry {
  lengths: number[];
  coordinates: number[];
}

type Ring = GeoJSON.Position[];

export function decodeEsriPbfGeometry(
  bytes: Uint8Array,
  transform: QuantizationTransform,
  geometryType: XZDisplayMetadata["geometryType"],
):
  | GeoJSON.Polygon
  | GeoJSON.MultiPolygon
  | GeoJSON.LineString
  | GeoJSON.MultiLineString
  | null {
  const pbf = parsePbfGeometry(bytes);
  const parts = unquantizeRings(pbf, transform);
  if (geometryType === "polyline") {
    const lines = parts.filter((part) => part.length >= 2);
    if (lines.length === 0) {
      return null;
    }
    return lines.length === 1
      ? { type: "LineString", coordinates: lines[0] }
      : { type: "MultiLineString", coordinates: lines };
  }

  const rings = parts
    .map(closeValidRing)
    .filter((ring): ring is Ring => ring !== null);
  const polygons = groupPolygonRings(rings);

  if (polygons.length === 0) {
    return null;
  }
  if (polygons.length === 1) {
    return {
      type: "Polygon",
      coordinates: polygons[0],
    };
  }

  return {
    type: "MultiPolygon",
    coordinates: polygons,
  };
}

function parsePbfGeometry(bytes: Uint8Array): PbfGeometry {
  const reader = new ProtobufReader(bytes);
  const lengths: number[] = [];
  const coordinates: number[] = [];

  while (!reader.done) {
    const tag = reader.readUnsignedVarint();
    const fieldNumber = Number(tag >> 3n);
    const wireType = Number(tag & 0b111n);

    if (fieldNumber === 2) {
      if (wireType === 2) {
        lengths.push(...reader.readPackedUnsigned());
      } else if (wireType === 0) {
        lengths.push(Number(reader.readUnsignedVarint()));
      } else {
        throw new Error("PBF geometry lengths use an unsupported wire type.");
      }
    } else if (fieldNumber === 3) {
      if (wireType === 2) {
        coordinates.push(...reader.readPackedSigned());
      } else if (wireType === 0) {
        coordinates.push(decodeZigzag(reader.readUnsignedVarint()));
      } else {
        throw new Error("PBF geometry coordinates use an unsupported wire type.");
      }
    } else {
      reader.skipField(wireType);
    }
  }

  const expectedCoordinateCount =
    lengths.reduce((sum, length) => sum + length, 0) * 2;
  if (coordinates.length !== expectedCoordinateCount) {
    throw new Error(
      `PBF geometry contains ${coordinates.length} ordinates, expected ${expectedCoordinateCount}.`,
    );
  }

  return { lengths, coordinates };
}

function unquantizeRings(
  geometry: PbfGeometry,
  transform: QuantizationTransform,
): Ring[] {
  const rings: Ring[] = [];
  let coordinateOffset = 0;

  for (const length of geometry.lengths) {
    let quantizedX = 0;
    let quantizedY = 0;
    const ring: Ring = [];

    for (let vertexIndex = 0; vertexIndex < length; vertexIndex += 1) {
      quantizedX += geometry.coordinates[coordinateOffset];
      quantizedY += geometry.coordinates[coordinateOffset + 1];
      coordinateOffset += 2;
      ring.push([
        quantizedX * transform.scale[0] + transform.translate[0],
        quantizedY * transform.scale[1] + transform.translate[1],
      ]);
    }

    rings.push(ring);
  }

  return rings;
}

function closeValidRing(ring: Ring): Ring | null {
  if (ring.length < 3) {
    return null;
  }

  const first = ring[0];
  const last = ring.at(-1)!;
  const closedRing =
    first[0] === last[0] && first[1] === last[1]
      ? ring
      : [...ring, [...first]];

  return closedRing.length >= 4 && signedRingArea(closedRing) !== 0
    ? closedRing
    : null;
}

function groupPolygonRings(rings: Ring[]): Ring[][] {
  const polygons: Ring[][] = [];

  for (const ring of rings) {
    const area = signedRingArea(ring);
    if (area < 0) {
      polygons.push([normalizeRingWinding(ring, false)]);
    } else {
      const polygon = polygons.at(-1);
      if (polygon) {
        polygon.push(normalizeRingWinding(ring, true));
      }
    }
  }

  return polygons;
}

function normalizeRingWinding(ring: Ring, clockwise: boolean): Ring {
  const isClockwise = signedRingArea(ring) < 0;
  return isClockwise === clockwise ? ring : [...ring].reverse();
}

function signedRingArea(ring: Ring): number {
  let area = 0;
  for (let index = 0; index < ring.length - 1; index += 1) {
    const current = ring[index];
    const next = ring[index + 1];
    area += current[0] * next[1] - next[0] * current[1];
  }

  return area / 2;
}

class ProtobufReader {
  private offset = 0;

  constructor(private readonly bytes: Uint8Array) {}

  get done(): boolean {
    return this.offset === this.bytes.length;
  }

  readUnsignedVarint(): bigint {
    let value = 0n;
    let shift = 0n;

    while (this.offset < this.bytes.length && shift <= 63n) {
      const byte = this.bytes[this.offset];
      this.offset += 1;
      value |= BigInt(byte & 0x7f) << shift;
      if ((byte & 0x80) === 0) {
        return value;
      }
      shift += 7n;
    }

    throw new Error("PBF geometry contains a truncated or oversized varint.");
  }

  readPackedUnsigned(): number[] {
    return this.readPacked((value) => Number(value));
  }

  readPackedSigned(): number[] {
    return this.readPacked(decodeZigzag);
  }

  skipField(wireType: number): void {
    if (wireType === 0) {
      this.readUnsignedVarint();
    } else if (wireType === 1) {
      this.advance(8);
    } else if (wireType === 2) {
      this.advance(Number(this.readUnsignedVarint()));
    } else if (wireType === 5) {
      this.advance(4);
    } else {
      throw new Error(`PBF geometry uses unsupported wire type ${wireType}.`);
    }
  }

  private readPacked(convert: (value: bigint) => number): number[] {
    const byteLength = Number(this.readUnsignedVarint());
    const end = this.offset + byteLength;
    if (!Number.isSafeInteger(byteLength) || end > this.bytes.length) {
      throw new Error("PBF geometry contains a truncated packed field.");
    }

    const values: number[] = [];
    while (this.offset < end) {
      values.push(convert(this.readUnsignedVarint()));
    }
    if (this.offset !== end) {
      throw new Error("PBF geometry packed field ended at an invalid offset.");
    }

    return values;
  }

  private advance(byteLength: number): void {
    const nextOffset = this.offset + byteLength;
    if (!Number.isSafeInteger(byteLength) || nextOffset > this.bytes.length) {
      throw new Error("PBF geometry contains a truncated field.");
    }
    this.offset = nextOffset;
  }
}

function decodeZigzag(value: bigint): number {
  const decoded = (value >> 1n) ^ -(value & 1n);
  const number = Number(decoded);
  if (!Number.isSafeInteger(number)) {
    throw new Error("PBF geometry coordinate exceeds JavaScript integer precision.");
  }

  return number;
}
