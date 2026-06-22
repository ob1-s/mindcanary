import { describe, expect, it } from "vitest";

import { observeTabRetention } from "./tab-retention";

describe("tab retention observer", () => {
  it("starts a private count baseline without emitting a metric", () => {
    const observed = observeTabRetention(
      undefined,
      12,
      Date.UTC(2026, 5, 15, 18, 0, 0),
      "UTC",
    );

    expect(observed).toEqual({
      state: {
        localDate: "2026-06-15",
        openTabCount: 12,
      },
    });
  });

  it("emits the carryover count on the first observation of a new day", () => {
    const observed = observeTabRetention(
      {
        localDate: "2026-06-15",
        openTabCount: 30,
      },
      24,
      Date.UTC(2026, 5, 16, 9, 0, 0),
      "UTC",
    );

    expect(observed).toEqual({
      state: {
        localDate: "2026-06-16",
        openTabCount: 24,
      },
      metric: {
        signal: "browser.retained_across_day_count",
        value: 24,
      },
    });
  });

  it("caps retention by yesterday's count when today's count grows", () => {
    const observed = observeTabRetention(
      {
        localDate: "2026-06-15",
        openTabCount: 9,
      },
      14,
      Date.UTC(2026, 5, 16, 9, 0, 0),
      "UTC",
    );

    expect(observed.metric?.value).toBe(9);
  });

  it("updates the same-day baseline without double counting", () => {
    const observed = observeTabRetention(
      {
        localDate: "2026-06-16",
        openTabCount: 24,
      },
      21,
      Date.UTC(2026, 5, 16, 12, 0, 0),
      "UTC",
    );

    expect(observed).toEqual({
      state: {
        localDate: "2026-06-16",
        openTabCount: 21,
      },
    });
  });
});
