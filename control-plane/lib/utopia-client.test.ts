import { describe, expect, it } from "vitest";

import {
  normalizeInternalBaseUrl,
  parseTickResponse,
  shouldContinueDrain,
} from "./utopia-client";

describe("normalizeInternalBaseUrl", () => {
  it("normalizes a service binding URL without losing its origin", () => {
    expect(normalizeInternalBaseUrl("https://utopia.internal/"))
      .toBe("https://utopia.internal");
  });

  it("rejects missing or non-http URLs", () => {
    expect(() => normalizeInternalBaseUrl(undefined)).toThrow(/UTOPIA_INTERNAL_URL/);
    expect(() => normalizeInternalBaseUrl("file:///tmp/utopia")).toThrow(/http/i);
  });
});

describe("parseTickResponse", () => {
  it("accepts the Rust hosted tick response shape", () => {
    expect(
      parseTickResponse({
        scheduled_sources: 1,
        scheduled_inference: 2,
        recovered_stale: 0,
        processed: 1,
        due_remaining: true,
      }),
    ).toEqual({
      scheduled_sources: 1,
      scheduled_inference: 2,
      recovered_stale: 0,
      processed: 1,
      due_remaining: true,
    });
  });

  it("rejects malformed or negative counters", () => {
    expect(() => parseTickResponse({ due_remaining: true })).toThrow(/tick response/i);
    expect(() =>
      parseTickResponse({
        scheduled_sources: 0,
        scheduled_inference: 0,
        recovered_stale: -1,
        processed: 0,
        due_remaining: false,
      }),
    ).toThrow(/tick response/i);
  });
});

describe("shouldContinueDrain", () => {
  it("continues only while work remains and the round cap is not reached", () => {
    expect(shouldContinueDrain({ due_remaining: true }, 0, 8)).toBe(true);
    expect(shouldContinueDrain({ due_remaining: false }, 0, 8)).toBe(false);
    expect(shouldContinueDrain({ due_remaining: true }, 7, 8)).toBe(false);
  });
});
