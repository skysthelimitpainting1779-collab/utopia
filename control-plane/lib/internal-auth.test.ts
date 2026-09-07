import { describe, expect, it } from "vitest";

import { isAuthorized } from "./internal-auth";

describe("isAuthorized", () => {
  it("accepts only an exact bearer token", () => {
    expect(isAuthorized("Bearer expected", "expected")).toBe(true);
    expect(isAuthorized("Bearer wrong", "expected")).toBe(false);
    expect(isAuthorized(undefined, "expected")).toBe(false);
    expect(isAuthorized("expected", "expected")).toBe(false);
  });

  it("fails closed when the configured token is empty", () => {
    expect(isAuthorized("Bearer expected", "")).toBe(false);
    expect(isAuthorized("Bearer expected", undefined)).toBe(false);
  });
});
