import { describe, expect, it } from "vitest";

import { parsePresignRequest } from "./blob-presign";

const SHA = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

describe("parsePresignRequest", () => {
  it("accepts one exact content-addressed Blob operation", () => {
    expect(
      parsePresignRequest({ pathname: `files/${SHA}`, operation: "put" }),
    ).toEqual({ pathname: `files/${SHA}`, operation: "put" });
  });

  it.each(["get", "head", "put", "delete"] as const)(
    "accepts the %s operation",
    (operation) => {
      expect(parsePresignRequest({ pathname: `files/${SHA}`, operation })).toEqual({
        pathname: `files/${SHA}`,
        operation,
      });
    },
  );

  it("rejects paths outside files/{lowercase sha256}", () => {
    expect(() =>
      parsePresignRequest({ pathname: "files/../secret", operation: "get" }),
    ).toThrow(/pathname/i);
    expect(() =>
      parsePresignRequest({ pathname: `files/${SHA.toUpperCase()}`, operation: "get" }),
    ).toThrow(/pathname/i);
  });

  it("rejects unrecognized operations", () => {
    expect(() =>
      parsePresignRequest({ pathname: `files/${SHA}`, operation: "list" }),
    ).toThrow(/operation/i);
  });
});
