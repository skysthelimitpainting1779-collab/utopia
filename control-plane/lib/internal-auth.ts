import { timingSafeEqual } from "node:crypto";

import { isAuthorizedGitHubPreview } from "./github-oidc";

function safeEqual(actual: string, expected: string): boolean {
  const actualBytes = Buffer.from(actual, "utf8");
  const expectedBytes = Buffer.from(expected, "utf8");
  if (actualBytes.length !== expectedBytes.length) return false;
  return timingSafeEqual(actualBytes, expectedBytes);
}

export function isAuthorized(
  authorization: string | null | undefined,
  expectedToken: string | null | undefined,
): boolean {
  const expected = expectedToken?.trim();
  if (!expected) return false;
  if (!authorization?.startsWith("Bearer ")) return false;
  const actual = authorization.slice("Bearer ".length);
  return safeEqual(actual, expected);
}

export function authorizeInternalRequest(request: Request): Response | null {
  if (
    isAuthorized(
      request.headers.get("authorization"),
      process.env.UTOPIA_CONTROL_PLANE_TOKEN,
    )
  ) {
    return null;
  }
  return Response.json({ error: "Unauthorized" }, { status: 401 });
}

export async function authorizeCronOrInternalRequest(
  request: Request,
): Promise<Response | null> {
  const authorization = request.headers.get("authorization");
  if (
    isAuthorized(authorization, process.env.UTOPIA_CONTROL_PLANE_TOKEN) ||
    isAuthorized(authorization, process.env.CRON_SECRET) ||
    (await isAuthorizedGitHubPreview(authorization))
  ) {
    return null;
  }
  return Response.json({ error: "Unauthorized" }, { status: 401 });
}
