import { sleep } from "workflow";

import {
  shouldContinueDrain,
  tickUtopia,
  type UtopiaTickResponse,
} from "../lib/utopia-client";

export const MAX_DRAIN_ROUNDS = 8;

export type DrainSummary = {
  rounds: number;
  capped: boolean;
  scheduled_sources: number;
  scheduled_inference: number;
  recovered_stale: number;
  processed: number;
  due_remaining: boolean;
};

async function tickUtopiaStep(): Promise<UtopiaTickResponse> {
  "use step";
  return tickUtopia({ maxJobs: 4, leaseSeconds: 900 });
}

export async function drainUtopia(): Promise<DrainSummary> {
  "use workflow";

  const summary: DrainSummary = {
    rounds: 0,
    capped: false,
    scheduled_sources: 0,
    scheduled_inference: 0,
    recovered_stale: 0,
    processed: 0,
    due_remaining: false,
  };

  for (let round = 0; round < MAX_DRAIN_ROUNDS; round += 1) {
    const result = await tickUtopiaStep();
    summary.rounds += 1;
    summary.scheduled_sources += result.scheduled_sources;
    summary.scheduled_inference += result.scheduled_inference;
    summary.recovered_stale += result.recovered_stale;
    summary.processed += result.processed;
    summary.due_remaining = result.due_remaining;

    if (!shouldContinueDrain(result, round, MAX_DRAIN_ROUNDS)) {
      summary.capped = result.due_remaining;
      return summary;
    }
    await sleep("2s");
  }

  summary.capped = summary.due_remaining;
  return summary;
}
