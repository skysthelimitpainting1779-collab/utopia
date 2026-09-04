import { describe, expect, it } from "vitest";

import { validateGitHubPreviewClaims } from "./github-oidc";

const now = 1_788_564_000;
const baseClaims = {
  iss: "https://token.actions.githubusercontent.com",
  aud: "https://github.com/skysthelimitpainting1779-collab",
  repository: "skysthelimitpainting1779-collab/utopia",
  repository_id: "1357530075",
  ref: "refs/heads/feat/vercel-hosted-mvp",
  sha: "abc123",
  workflow_ref:
    "skysthelimitpainting1779-collab/utopia/.github/workflows/hosted-ci.yml@refs/heads/feat/vercel-hosted-mvp",
  runner_environment: "github-hosted",
  event_name: "push",
  iat: now - 10,
  nbf: now - 10,
  exp: now + 300,
};

describe("validateGitHubPreviewClaims", () => {
  it("accepts only the hosted CI workflow for the exact preview SHA", () => {
    expect(validateGitHubPreviewClaims(baseClaims, "abc123", now)).toBe(true);
    expect(validateGitHubPreviewClaims(baseClaims, "other", now)).toBe(false);
  });

  it("rejects wrong repository, ref, workflow, audience, or expired tokens", () => {
    expect(
      validateGitHubPreviewClaims({ ...baseClaims, repository: "other/repo" }, "abc123", now),
    ).toBe(false);
    expect(validateGitHubPreviewClaims({ ...baseClaims, ref: "refs/heads/dev" }, "abc123", now)).toBe(false);
    expect(
      validateGitHubPreviewClaims({ ...baseClaims, workflow_ref: "other.yml" }, "abc123", now),
    ).toBe(false);
    expect(validateGitHubPreviewClaims({ ...baseClaims, aud: "wrong" }, "abc123", now)).toBe(false);
    expect(validateGitHubPreviewClaims({ ...baseClaims, exp: now - 31 }, "abc123", now)).toBe(false);
  });
});
