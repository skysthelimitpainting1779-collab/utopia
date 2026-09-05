import { start } from "workflow/api";

import { authorizeCronOrInternalRequest } from "../../../../lib/internal-auth";
import { drainUtopia } from "../../../../workflows/drain-utopia";

export const runtime = "nodejs";

async function startDrain(request: Request): Promise<Response> {
  console.info("Utopia cron auth environment presence", {
    controlPlaneToken: Boolean(process.env.UTOPIA_CONTROL_PLANE_TOKEN),
    cronSecret: Boolean(process.env.CRON_SECRET),
  });
  const denied = await authorizeCronOrInternalRequest(request);
  if (denied) return denied;

  try {
    const run = await start(drainUtopia, []);
    return Response.json({ runId: run.runId }, { status: 202 });
  } catch (error) {
    console.error(
      "Failed to start Utopia drain Workflow",
      error instanceof Error ? error.name : "UnknownError",
    );
    return Response.json({ error: "Workflow start failed" }, { status: 502 });
  }
}

export async function GET(request: Request): Promise<Response> {
  return startDrain(request);
}

export async function POST(request: Request): Promise<Response> {
  return startDrain(request);
}
