import { describe, expect, it } from "vitest";

import {
  DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT,
  MAX_DAILY_RHYTHM_INSIGHT_LIMIT,
  PROTOCOL_VERSION,
  type ProtocolResponse,
} from "@mindcanary/protocol";

import {
  createDailyRhythmInsightsRequest,
  toDailyRhythmDashboardModel,
} from "./insights";

const BLOCKED_LANGUAGE =
  /mania|manic|depression|depressive|psychosis|diagnosis|warning|risk|alert/i;

describe("daily rhythm insight dashboard model", () => {
  it("builds the typed daemon read request", () => {
    expect(createDailyRhythmInsightsRequest()).toEqual({
      type: "get_daily_rhythm_insights",
      protocol_version: PROTOCOL_VERSION,
      limit: DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT,
    });
    expect(createDailyRhythmInsightsRequest(5)).toEqual({
      type: "get_daily_rhythm_insights",
      protocol_version: PROTOCOL_VERSION,
      limit: 5,
    });
  });

  it("rejects invalid client-side insight limits", () => {
    expect(() => createDailyRhythmInsightsRequest(0)).toThrow(RangeError);
    expect(() =>
      createDailyRhythmInsightsRequest(MAX_DAILY_RHYTHM_INSIGHT_LIMIT + 1),
    ).toThrow(RangeError);
  });

  it("maps insight responses into neutral dashboard cards", () => {
    const model = toDailyRhythmDashboardModel({
      type: "daily_rhythm_insights",
      protocol_version: PROTOCOL_VERSION,
      generated_at: "2026-06-14T12:00:00Z",
      summary: {
        daily_snapshot_count: 5,
        browser_day_count: 5,
        os_day_count: 0,
        check_in_day_count: 5,
        insight_count_before_limit: 2,
        insights_truncated: false,
      },
      readiness: [
        {
          dimension: "browser_tabs",
          status: "change_described",
          comparable_day_count: 4,
          minimum_day_count: 3,
        },
        {
          dimension: "energy",
          status: "within_baseline",
          comparable_day_count: 4,
          minimum_day_count: 3,
        },
      ],
      insights: [
        {
          local_date: "2026-01-09",
          dimension: "browser_tabs",
          direction: "higher",
          summary:
            "Across a 2-day window, Open-tab average was higher than your prior personal baseline.",
          evidence: [
            { label: "current window median", value: "24 tabs" },
            {
              label: "current window",
              value: "2 days: 2026-01-08, 2026-01-09",
            },
            { label: "baseline median", value: "9.5 tabs" },
            {
              label: "prior dates",
              value: "2026-01-05, 2026-01-06, 2026-01-07",
            },
          ],
        },
      ],
    });

    expect(model.state).toBe("ready");
    expect(model.cards).toEqual([
      {
        localDate: "2026-01-09",
        dimensionLabel: "Browser tabs",
        changeLabel: "Higher than baseline",
        summary:
          "Across a 2-day window, Open-tab average was higher than your prior personal baseline.",
        evidence: [
          "current window median: 24 tabs",
          "current window: 2 days: 8 Jan 2026, 9 Jan 2026",
          "baseline median: 9.5 tabs",
          "prior dates: 5 Jan 2026, 6 Jan 2026, 7 Jan 2026",
        ],
      },
    ]);
    expect(model.readiness).toEqual([
      {
        dimensionLabel: "Browser tabs",
        statusLabel: "Change noted",
        detail: "Based on 4 earlier days.",
        state: "described",
      },
      {
        dimensionLabel: "Energy check-in",
        statusLabel: "Within your range",
        detail:
          "4 earlier days available; recent values stayed within your usual range.",
        state: "stable",
      },
    ]);
    expect(JSON.stringify(model)).not.toMatch(BLOCKED_LANGUAGE);
  });

  it("uses a calm empty state before baseline coverage is enough", () => {
    const response: ProtocolResponse = {
      type: "daily_rhythm_insights",
      protocol_version: PROTOCOL_VERSION,
      generated_at: "2026-06-14T12:00:00Z",
      summary: {
        daily_snapshot_count: 2,
        browser_day_count: 2,
        os_day_count: 0,
        check_in_day_count: 1,
        insight_count_before_limit: 0,
        insights_truncated: false,
      },
      readiness: [
        {
          dimension: "browser_tabs",
          status: "insufficient_baseline",
          comparable_day_count: 1,
          minimum_day_count: 3,
        },
        {
          dimension: "energy",
          status: "missing_current",
          comparable_day_count: 1,
          minimum_day_count: 3,
        },
      ],
      insights: [],
    };

    const model = toDailyRhythmDashboardModel(response);

    expect(model.state).toBe("empty");
    if (model.state !== "empty") {
      throw new Error("expected empty model");
    }
    expect(model.emptyTitle).toBe("The canary is listening");
    expect(model.emptyBody).toContain("comparisons begin");
    expect(model.baselineProgressText).toBe(
      "2 of 5 days logged. Gaps don't reset this.",
    );
    expect(model.readiness[0]?.detail).toBe(
      "1 of 3 days needed before comparisons can start.",
    );
    expect(model.readiness[1]?.statusLabel).toBe("No recent value");
    expect(JSON.stringify(model)).not.toMatch(BLOCKED_LANGUAGE);
  });

  it("describes an incomplete window as a waiting state", () => {
    const model = toDailyRhythmDashboardModel({
      type: "daily_rhythm_insights",
      protocol_version: PROTOCOL_VERSION,
      generated_at: "2026-06-14T12:00:00Z",
      summary: {
        daily_snapshot_count: 4,
        browser_day_count: 4,
        os_day_count: 0,
        check_in_day_count: 0,
        insight_count_before_limit: 0,
        insights_truncated: false,
      },
      readiness: [
        {
          dimension: "browser_tabs",
          status: "needs_sustained_change",
          comparable_day_count: 3,
          minimum_day_count: 3,
        },
      ],
      insights: [],
    });

    expect(model.state).toBe("empty");
    if (model.state !== "empty") {
      throw new Error("expected empty model");
    }
    expect(model.readiness[0]).toEqual({
      dimensionLabel: "Browser tabs",
      statusLabel: "No clear shift yet",
      detail: "3 earlier days available; no clear shift seen yet.",
      state: "waiting",
    });
    expect(JSON.stringify(model)).not.toMatch(BLOCKED_LANGUAGE);
  });

  it("shows variable baselines as a waiting state", () => {
    const model = toDailyRhythmDashboardModel({
      type: "daily_rhythm_insights",
      protocol_version: PROTOCOL_VERSION,
      generated_at: "2026-06-14T12:00:00Z",
      summary: {
        daily_snapshot_count: 5,
        browser_day_count: 5,
        os_day_count: 0,
        check_in_day_count: 0,
        insight_count_before_limit: 0,
        insights_truncated: false,
      },
      readiness: [
        {
          dimension: "browser_tabs",
          status: "unstable_baseline",
          comparable_day_count: 4,
          minimum_day_count: 3,
        },
      ],
      insights: [],
    });

    expect(model.state).toBe("empty");
    if (model.state !== "empty") {
      throw new Error("expected empty model");
    }
    expect(model.readiness[0]).toEqual({
      dimensionLabel: "Browser tabs",
      statusLabel: "History still varies",
      detail: "Earlier values varied too much for a useful comparison.",
      state: "waiting",
    });
    expect(JSON.stringify(model)).not.toMatch(BLOCKED_LANGUAGE);
  });
});
