import { describe, expect, it } from "vitest";

import { formatParquetKeyValueMetadata } from "./keyValueMetadata";

describe("formatParquetKeyValueMetadata", () => {
  it("expands JSON values and preserves strings and nulls", () => {
    const formatted = formatParquetKeyValueMetadata([
      { key: "geo", value: "{\"version\":\"1.1.0\"}" },
      { key: "created_by", value: "writer" },
      { key: "empty", value: null },
    ]);

    expect(JSON.parse(formatted)).toEqual({
      geo: { version: "1.1.0" },
      created_by: "writer",
      empty: null,
    });
  });
});
