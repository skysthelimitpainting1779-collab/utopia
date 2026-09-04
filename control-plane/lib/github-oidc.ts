import { createPublicKey, verify } from "node:crypto";

const ISSUER = "https://token.actions.githubusercontent.com";
const JWKS_URL = `${ISSUER}/.well-known/jwks`;
const EXPECTED_AUDIENCE = "https://github.com/skysthelimitpainting1779-collab";
const EXPECTED_REPOSITORY = "skysthelimitpainting1779-collab/utopia";
const EXPECTED_REPOSITORY_ID = "1357530075";
const EXPECTED_REF = "refs/heads/feat/vercel-hosted-mvp";
const EXPECTED_WORKFLOW_REF =
  `${EXPECTED_REPOSITORY}/.github/workflows/hosted-ci.yml@${EXPECTED_REF}`;

type JwtHeader = { alg?: unknown; kid?: unknown; typ?: unknown };

export type GitHubOidcClaims = {
  aud?: unknown;
  exp?: unknown;
  iat?: unknown;
  iss?: unknown;
  nbf?: unknown;
  repository?: unknown;
  repository_id?: unknown;
  ref?: unknown;
  runner_environment?: unknown;
  sha?: unknown;
  workflow_ref?: unknown;
  event_name?: unknown;
};

type Jwk = JsonWebKey & {
  alg?: string;
  kid?: string;
  kty?: string;
  use?: string;
};

let cachedJwks: { expiresAt: number; keys: Jwk[] } | undefined;

function decodeJson<T>(segment: string): T {
  return JSON.parse(Buffer.from(segment, "base64url").toString("utf8")) as T;
}

function audienceMatches(aud: unknown): boolean {
  if (typeof aud === "string") return aud === EXPECTED_AUDIENCE;
  return Array.isArray(aud) && aud.some((value) => value === EXPECTED_AUDIENCE);
}

export function validateGitHubPreviewClaims(
  claims: GitHubOidcClaims,
  expectedSha: string | undefined,
  nowSeconds = Math.floor(Date.now() / 1000),
): boolean {
  if (claims.iss !== ISSUER || !audienceMatches(claims.aud)) return false;
  if (claims.repository !== EXPECTED_REPOSITORY) return false;
  if (String(claims.repository_id ?? "") !== EXPECTED_REPOSITORY_ID) return false;
  if (claims.ref !== EXPECTED_REF) return false;
  if (!expectedSha || claims.sha !== expectedSha) return false;
  if (claims.workflow_ref !== EXPECTED_WORKFLOW_REF) return false;
  if (claims.runner_environment !== "github-hosted") return false;
  if (claims.event_name !== "push" && claims.event_name !== "workflow_dispatch") {
    return false;
  }
  if (
    typeof claims.exp !== "number" ||
    typeof claims.iat !== "number" ||
    typeof claims.nbf !== "number"
  ) {
    return false;
  }
  if (claims.exp < nowSeconds - 30) return false;
  if (claims.nbf > nowSeconds + 30) return false;
  if (claims.iat > nowSeconds + 30) return false;
  return true;
}

async function getSigningKey(kid: string): Promise<Jwk | undefined> {
  const now = Date.now();
  if (!cachedJwks || cachedJwks.expiresAt <= now) {
    const response = await fetch(JWKS_URL, {
      cache: "no-store",
      signal: AbortSignal.timeout(5_000),
    });
    if (!response.ok) return undefined;
    const body = (await response.json()) as { keys?: Jwk[] };
    if (!Array.isArray(body.keys)) return undefined;
    cachedJwks = { expiresAt: now + 5 * 60 * 1000, keys: body.keys };
  }
  return cachedJwks.keys.find(
    (key) =>
      key.kid === kid &&
      key.kty === "RSA" &&
      (!key.alg || key.alg === "RS256") &&
      (!key.use || key.use === "sig"),
  );
}

export async function isAuthorizedGitHubPreview(
  authorization: string | null | undefined,
): Promise<boolean> {
  if (process.env.VERCEL_ENV !== "preview") return false;
  if (!authorization?.startsWith("Bearer ")) return false;

  const token = authorization.slice("Bearer ".length).trim();
  const parts = token.split(".");
  if (parts.length !== 3 || parts.some((part) => !part)) return false;

  try {
    const header = decodeJson<JwtHeader>(parts[0]);
    if (header.alg !== "RS256" || typeof header.kid !== "string") return false;

    const claims = decodeJson<GitHubOidcClaims>(parts[1]);
    if (!validateGitHubPreviewClaims(claims, process.env.VERCEL_GIT_COMMIT_SHA?.trim())) {
      return false;
    }

    const jwk = await getSigningKey(header.kid);
    if (!jwk) return false;
    const key = createPublicKey({ key: jwk, format: "jwk" });
    return verify(
      "RSA-SHA256",
      Buffer.from(`${parts[0]}.${parts[1]}`, "utf8"),
      key,
      Buffer.from(parts[2], "base64url"),
    );
  } catch {
    return false;
  }
}
