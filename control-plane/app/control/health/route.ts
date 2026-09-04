export const runtime = "nodejs";

export async function GET(): Promise<Response> {
  return Response.json({
    status: "ok",
    name: "utopia-control-plane",
    commit: process.env.VERCEL_GIT_COMMIT_SHA ?? null,
  });
}
