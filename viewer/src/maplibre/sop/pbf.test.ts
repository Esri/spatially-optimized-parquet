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

import { describe, expect, it } from "vitest";

import type { QuantizationTransform } from "./metadata";
import { decodeGeometry } from "./pbf";

const identityTransform: QuantizationTransform = {
  scale: [1, 1, 1, 1],
  translate: [0, 0, 0, 0],
};

describe("decodeGeometry PBF fields", () => {
  it("decodes packed lengths and signed coordinate deltas", () => {
    const bytes = encodeGeometry(
      [3],
      [1, 2, 2, -1, -1, 3],
    );
    const transform: QuantizationTransform = {
      scale: [2, 3, 1, 1],
      translate: [10, -5, 0, 0],
    };

    expect(decodeGeometry(bytes, transform, "polyline")).toEqual({
      type: "LineString",
      coordinates: [
        [12, 1],
        [16, -2],
        [14, 7],
      ],
    });
  });

  it("accepts unpacked repeated lengths and coordinates", () => {
    const bytes = encodeGeometry(
      [2, 2],
      [1, 1, 1, 0, 3, 2, -1, 1],
      false,
    );

    expect(decodeGeometry(bytes, identityTransform, "polyline")).toEqual({
      type: "MultiLineString",
      coordinates: [
        [
          [1, 1],
          [2, 1],
        ],
        [
          [3, 2],
          [2, 3],
        ],
      ],
    });
  });

  it("skips unknown scalar and length-delimited fields", () => {
    const bytes = Uint8Array.from([
      ...encodeField(1, 0, encodeVarint(150n)),
      ...encodeField(4, 1, new Array(8).fill(0)),
      ...encodeGeometry([2], [1, 1, 2, 3]),
      ...encodeField(5, 2, [3, 7, 8, 9]),
      ...encodeField(6, 5, new Array(4).fill(0)),
    ]);

    expect(decodeGeometry(bytes, identityTransform, "polyline")).toEqual({
      type: "LineString",
      coordinates: [
        [1, 1],
        [3, 4],
      ],
    });
  });

  it.each([
    {
      name: "coordinate count mismatch",
      bytes: encodeGeometry([2], [1, 1]),
      message: "contains 2 ordinates, expected 4",
    },
    {
      name: "unsupported lengths wire type",
      bytes: Uint8Array.from(encodeField(2, 5, [0, 0, 0, 0])),
      message: "lengths use an unsupported wire type",
    },
    {
      name: "unsupported coordinates wire type",
      bytes: Uint8Array.from([
        ...encodePackedField(2, [1n]),
        ...encodeField(3, 5, [0, 0, 0, 0]),
      ]),
      message: "coordinates use an unsupported wire type",
    },
    {
      name: "truncated varint",
      bytes: Uint8Array.from([0x12, 0x80]),
      message: "truncated or oversized varint",
    },
    {
      name: "oversized varint",
      bytes: Uint8Array.from(new Array(10).fill(0x80)),
      message: "truncated or oversized varint",
    },
    {
      name: "truncated packed field",
      bytes: Uint8Array.from([0x12, 0x02, 0x01]),
      message: "truncated packed field",
    },
    {
      name: "varint crossing a packed boundary",
      bytes: Uint8Array.from([0x12, 0x01, 0x80, 0x00]),
      message: "packed field ended at an invalid offset",
    },
    {
      name: "unsupported unknown wire type",
      bytes: Uint8Array.from([0x23]),
      message: "unsupported wire type 3",
    },
    {
      name: "truncated unknown fixed-width field",
      bytes: Uint8Array.from([0x21, 0, 0, 0]),
      message: "contains a truncated field",
    },
    {
      name: "coordinate beyond exact JavaScript precision",
      bytes: encodeGeometry([1], [9_007_199_254_740_992n, 0]),
      message: "coordinate exceeds JavaScript integer precision",
    },
  ])("rejects $name", ({ bytes, message }) => {
    expect(() =>
      decodeGeometry(bytes, identityTransform, "polyline"),
    ).toThrow(message);
  });
});

describe("decodeGeometry parts", () => {
  it("resets XY delta accumulation for each polyline part", () => {
    const bytes = encodeGeometry(
      [2, 2],
      [5, 6, 1, 2, 20, 30, -2, -3],
    );
    const transform: QuantizationTransform = {
      scale: [0.5, 2, 1, 1],
      translate: [-10, 100, 0, 0],
    };

    expect(decodeGeometry(bytes, transform, "polyline")).toEqual({
      type: "MultiLineString",
      coordinates: [
        [
          [-7.5, 112],
          [-7, 116],
        ],
        [
          [0, 160],
          [-1, 154],
        ],
      ],
    });
  });

  it("drops degenerate polyline parts and returns null when none remain", () => {
    expect(
      decodeGeometry(
        encodeGeometry([1, 2], [9, 9, 1, 2, 3, 4]),
        identityTransform,
        "polyline",
      ),
    ).toEqual({
      type: "LineString",
      coordinates: [
        [1, 2],
        [4, 6],
      ],
    });

    expect(
      decodeGeometry(
        encodeGeometry([0, 1], [5, 5]),
        identityTransform,
        "polyline",
      ),
    ).toBeNull();
  });

  it("closes polygon rings and normalizes exterior and hole winding", () => {
    const bytes = encodeGeometry(
      [4, 4],
      [
        0, 0, 0, 4, 4, 0, 0, -4,
        1, 1, 2, 0, 0, 2, -2, 0,
      ],
    );
    const transform: QuantizationTransform = {
      scale: [2, 3, 1, 1],
      translate: [10, -5, 0, 0],
    };

    expect(decodeGeometry(bytes, transform, "polygon")).toEqual({
      type: "Polygon",
      coordinates: [
        [
          [10, -5],
          [18, -5],
          [18, 7],
          [10, 7],
          [10, -5],
        ],
        [
          [12, -2],
          [12, 4],
          [16, 4],
          [16, -2],
          [12, -2],
        ],
      ],
    });
  });

  it("starts a new polygon for each clockwise exterior ring", () => {
    const bytes = encodeGeometry(
      [4, 4],
      [
        0, 0, 0, 2, 2, 0, 0, -2,
        5, 5, 0, 2, 2, 0, 0, -2,
      ],
    );

    expect(decodeGeometry(bytes, identityTransform, "polygon")).toEqual({
      type: "MultiPolygon",
      coordinates: [
        [[
          [0, 0],
          [2, 0],
          [2, 2],
          [0, 2],
          [0, 0],
        ]],
        [[
          [5, 5],
          [7, 5],
          [7, 7],
          [5, 7],
          [5, 5],
        ]],
      ],
    });
  });

  it("drops short and zero-area polygon rings", () => {
    expect(
      decodeGeometry(
        encodeGeometry(
          [2, 4],
          [0, 0, 1, 1, 0, 0, 1, 1, 1, 1, -1, -1],
        ),
        identityTransform,
        "polygon",
      ),
    ).toBeNull();
  });
});

function encodeGeometry(
  lengths: number[],
  coordinateDeltas: Array<number | bigint>,
  packed = true,
): Uint8Array {
  const encodedLengths = lengths.map(BigInt);
  const encodedCoordinates = coordinateDeltas.map(encodeZigzag);

  if (packed) {
    return Uint8Array.from([
      ...encodePackedField(2, encodedLengths),
      ...encodePackedField(3, encodedCoordinates),
    ]);
  }

  return Uint8Array.from([
    ...encodedLengths.flatMap((value) =>
      encodeField(2, 0, encodeVarint(value)),
    ),
    ...encodedCoordinates.flatMap((value) =>
      encodeField(3, 0, encodeVarint(value)),
    ),
  ]);
}

function encodePackedField(fieldNumber: number, values: bigint[]): number[] {
  const payload = values.flatMap(encodeVarint);
  return encodeField(fieldNumber, 2, [
    ...encodeVarint(BigInt(payload.length)),
    ...payload,
  ]);
}

function encodeField(
  fieldNumber: number,
  wireType: number,
  payload: number[],
): number[] {
  return [
    ...encodeVarint(BigInt((fieldNumber << 3) | wireType)),
    ...payload,
  ];
}

function encodeVarint(value: bigint): number[] {
  const bytes: number[] = [];
  let remaining = value;

  do {
    let byte = Number(remaining & 0x7fn);
    remaining >>= 7n;
    if (remaining !== 0n) {
      byte |= 0x80;
    }
    bytes.push(byte);
  } while (remaining !== 0n);

  return bytes;
}

function encodeZigzag(value: number | bigint): bigint {
  const integer = BigInt(value);
  return integer >= 0n ? integer * 2n : -integer * 2n - 1n;
}
