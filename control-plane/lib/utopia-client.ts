export type UtopiaTickResponse = {
  scheduled_sources: number;
  scheduled_inference: number;
  recovered_stale: number;
  processed: number;
  due_remaining: boolean;
};

const INTEGER_FIELDS = [
  "scheduled_sources",
  "scheduled_inference",
  "recovered_stale",
  "processed",
] as const;

function isRecord(input: unknown): input is Record<string, unknown> {
  return typeof input === "object" && input !== null && !Array.isArray(input);
}

export function normalizeInternalBaseUrl(raw: string | undefined): string {
  if (!raw?.trim()) {
    throw new Error("UTOPIA_INTERNAL_URL is required for the hosted control plane");
  }
  const parsed = new URL(raw.trim());
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error("UTOPIA_INTERNAL_URL must use http or https");
  }
  parsed.pathname = "";
  parsed.search = "";
  parsed.hash = "";
  return parsed.toString().replace(/\/$/, "");
}

export function parseTickResponse(input: unknown): UtopiaTickResponse {
  if (!isRecord(input) || typeof input.due_remaining !== "boolean") {
    throw new Error("Invalid Utopia tick response");
  }
  for (const field of INTEGER_FIELDS) {
    const value = input[field];
    if (!Number.isSafeInteger(value) || (value as number) < 0) {
      throw new Error("Invalid Utopia tick response");
    }
  }
  return {
    scheduled_sources: input.scheduled_sources as number,
    scheduled_inference: input.scheduled_inference as number,
    recovered_stale: input.recovered_stale as number,
    processed: input.processed as number,
    due_remaining: input.due_remaining,
  };
}

export function shouldContinueDrain(
  response: Pick<UtopiaTickResponse, "due_remaining">,
  roundIndex: number,
  maxRounds: number,
): boolean {
  return response.due_remaining && roundIndex + 1 < maxRounds;
}

function internalBaseUrl(): string {
  const explicit = process.env.UTOPIA_INTERNAL_URL;
  if (explicit) return normalizeInternalBaseUrl(explicit);
  const deployment = process.env.VERCEL_URL;
  return normalizeInternalBaseUrl(deployment ? `https://${deployment}` : undefined);
}

export async function tickUtopia(options?: {
  maxJobs?: number;
  leaseSeconds?: number;
}): Promise<UtopiaTickResponse> {
  const token = process.env.UTOPIA_CONTROL_PLANE_TOKEN?.trim();
  if (!token) throw new Error("UTOPIA_CONTROL_PLANE_TOKEN is required");

  const url = new URL("/_internal/hosted/tick", internalBaseUrl());
  url.searchParams.set("max_jobs", String(options?.maxJobs ?? 4));
  url.searchParams.set("lease_seconds", String(options?.leaseSeconds ?? 900));

  const response = await fetch(url, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      accept: "application/json",
    },
    cache: "no-store",
  });
  if (!response.ok) {
    const detail = (await response.text()).slice(0, 500);
    throw new Error(
      `Utopia hosted tick failed with HTTP ${response.status}${detail ? `: ${detail}` : ""}`,
    );
  }
  return parseTickResponse(await response.json());
}
