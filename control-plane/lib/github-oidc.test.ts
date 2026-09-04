import { describe, expect, it } from "vitest";

import { validateGitHubPreviewClaims, type GitHubOidcClaims } from "./github-oidc";

const now = 1_800_000_000;
const sha = "a".repeat(40);
const hostedCi =
  "skysthelimitpainting1779-collab/utopia/.github/workflows/hosted-ci.yml@refs/heads/feat/vercel-hosted-mvp";
const hostedFullSmoke =
  "skysthelimitpainting1779-collab/utopia/.github/workflows/hosted-full-smoke.yml@refs/heads/feat/vercel-hosted-mvp";

function validClaims(workflowRef = hostedCi): GitHubOidcClaims {
  return {
    iss: "https://token.actions.githubusercontent.com",
    aud: "https://github.com/skysthelimitpainting1779-collab",
    repository: "skysthelimitpainting1779-collab/utopia",
    repository_id: "1357530075",
    ref: "refs/heads/feat/vercel-hosted-mvp",
    workflow_ref: workflowRef,
    runner_environment: "github-hosted",
    event_name: "push",
    sha,
    iat: now - 5,
    nbf: now - 5,
    exp: now + 300,
  };
}

describe("validateGitHubPreviewClaims", () => {
  it("accepts only the two hosted workflows on this branch/repository/exact sha", () => {
    expect(validateGitHubPreviewClaims(validClaims(hostedCi), sha, now)).toBe(true);
    expect(validateGitHubPreviewClaims(validClaims(hostedFullSmoke), sha, now)).toBe(true);

    for (const patch of [
      { repository: "someone/else" },
      { ref: "refs/heads/dev" },
      { workflow_ref: "skysthelimitpainting1779-collab/utopia/.github/workflows/ci.yml@refs/heads/feat/vercel-hosted-mvp" },
      { sha: "b".repeat(40) },
      { aud: "wrong" },
      { event_name: "pull_request" },
    ]) {
      expect(validateGitHubPreviewClaims({ ...validClaims(), ...patch }, sha, now)).toBe(false);
    }
  });

  it("rejects expired and not-yet-valid credentials", () => {
    expect(validateGitHubPreviewClaims({ ...validClaims(), exp: now - 31 }, sha, now)).toBe(false);
    expect(validateGitHubPreviewClaims({ ...validClaims(), nbf: now + 31 }, sha, now)).toBe(false);
  });
});
