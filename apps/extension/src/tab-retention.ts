import type { Metric } from "@mindcanary/protocol";

export const TAB_RETENTION_SIGNAL = "browser.retained_across_day_count";

export interface TabRetentionState {
  localDate: string;
  openTabCount: number;
}

export interface TabRetentionObservation {
  state: TabRetentionState;
  metric?: Metric;
}

export function observeTabRetention(
  previous: TabRetentionState | undefined,
  openTabCount: number,
  observedAtMs: number,
  timeZone: string,
): TabRetentionObservation {
  const current = {
    localDate: localDateKey(observedAtMs, timeZone),
    openTabCount: nonNegativeInteger(openTabCount),
  };

  if (previous === undefined || previous.localDate === current.localDate) {
    return { state: current };
  }

  return {
    state: current,
    metric: {
      signal: TAB_RETENTION_SIGNAL,
      value: Math.min(previous.openTabCount, current.openTabCount),
    },
  };
}

function localDateKey(timestampMs: number, timeZone: string): string {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(timestampMs);
  const values = Object.fromEntries(
    parts.flatMap((part) =>
      part.type === "year" || part.type === "month" || part.type === "day"
        ? [[part.type, part.value]]
        : [],
    ),
  );

  return `${values.year}-${values.month}-${values.day}`;
}

function nonNegativeInteger(value: number): number {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("tab retention counts must be non-negative integers");
  }
  return value;
}
