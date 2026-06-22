import { describe, expect, it } from "vitest";

import {
  DEFAULT_DAILY_TIMELINE_LIMIT,
  MAX_DAILY_TIMELINE_LIMIT,
  PROTOCOL_VERSION,
  type ProtocolResponse,
} from "@mindcanary/protocol";

import {
  createDailyTimelineRequest,
  toDailyTimelineDashboardModel,
  toLatestLocalRecordModel,
  toPriorCheckInReferences,
} from "./timeline";

const BLOCKED_LANGUAGE =
  /mania|manic|depression|depressive|psychosis|diagnosis|warning|risk|alert/i;

describe("daily timeline dashboard model", () => {
  it("builds and validates the typed timeline request", () => {
    expect(createDailyTimelineRequest()).toEqual({
      type: "get_daily_timeline",
      protocol_version: PROTOCOL_VERSION,
      limit: DEFAULT_DAILY_TIMELINE_LIMIT,
    });
    expect(createDailyTimelineRequest(7)).toEqual({
      type: "get_daily_timeline",
      protocol_version: PROTOCOL_VERSION,
      limit: 7,
    });
    expect(() => createDailyTimelineRequest(0)).toThrow(RangeError);
    expect(() =>
      createDailyTimelineRequest(MAX_DAILY_TIMELINE_LIMIT + 1),
    ).toThrow(RangeError);
  });

  it("keeps browser, OS, check-in, and missing coverage explicit", () => {
    const response: ProtocolResponse = {
      type: "daily_timeline",
      protocol_version: PROTOCOL_VERSION,
      generated_at: "2026-06-14T12:00:00Z",
      summary: {
        calendar_day_count_before_limit: 3,
        returned_day_count: 3,
        browser_day_count: 1,
        os_day_count: 1,
        check_in_day_count: 1,
        annotation_day_count: 1,
        missing_day_count: 1,
        days_truncated: false,
      },
      days: [
        {
          local_date: "2026-06-12",
          browser: {
            open_tab_count_mean: 12.5,
            open_tab_count_max: 20,
            tab_switch_count: 42,
            retained_across_day_count: 8,
            active_seconds: 3600,
            idle_seconds: 900,
            recorded_bucket_count: 4,
          },
          os: null,
          check_in: null,
          annotations: [],
        },
        {
          local_date: "2026-06-13",
          browser: null,
          os: null,
          check_in: null,
          annotations: [],
        },
        {
          local_date: "2026-06-14",
          browser: null,
          os: {
            active_seconds: 5400,
            idle_seconds: 1200,
            lock_count: 1,
            unlock_count: 1,
            suspend_count: 0,
            resume_count: 0,
            recorded_bucket_count: 6,
          },
          check_in: {
            sleep_minutes: 420,
            mood: 5,
            energy: 6,
            irritability: 2,
            concentration: 4,
            impulsivity: 3,
            check_in_count: 2,
            context_tags: ["deadline", "news_cycle"],
          },
          annotations: [
            {
              annotation_id: "annotation-id",
              created_at: "2026-06-14T12:00:00Z",
              time_zone: "America/Sao_Paulo",
              local_date: "2026-06-14",
              start_minute: 780,
              end_minute: 870,
              text: "Afternoon nap",
              context_tags: ["other"],
            },
          ],
        },
      ],
    };

    const model = toDailyTimelineDashboardModel(response);

    expect(model.state).toBe("ready");
    if (model.state !== "ready") {
      throw new Error("expected ready model");
    }
    expect(model.days.map((day) => day.coverageLabel)).toEqual([
      "Browser",
      "No record",
      "OS + Check-in + Annotation",
    ]);
    expect(model.days[0]?.browser?.summary).toBe(
      "12.5 average open tabs · 42 tab switches · 1h active",
    );
    expect(model.days[0]?.browser?.retainedAcrossDays).toBe(8);
    expect(model.days[2]?.checkIn?.contextLabels).toEqual([
      "Deadline",
      "News cycle",
    ]);
    expect(model.days[2]?.os?.summary).toBe(
      "1.5h computer active · 1 lock · 0 suspends",
    );
    expect(model.days[2]?.annotations[0]).toMatchObject({
      text: "Afternoon nap",
      windowLabel: "13:00-14:30",
      contextLabels: ["Other"],
    });
    expect(model.coverageText).toContain("1 explicit gap");
    expect(toLatestLocalRecordModel(model)).toEqual({
      state: "ready",
      dateLabel: "Sun, Jun 14",
      coverageLabel: "OS + Check-in + Annotation",
      entries: [
        {
          label: "Computer",
          summary: "1.5h computer active · 1 lock · 0 suspends",
        },
        {
          label: "Check-in",
          summary: "2 check-ins summarized · 7h sleep · energy 6/7 · mood 5/7",
        },
        { label: "Context", summary: "1 private annotation" },
      ],
    });
    expect(toPriorCheckInReferences(model, "2026-06-15")).toEqual({
      mood: { median: 5, dayCount: 1 },
      energy: { median: 6, dayCount: 1 },
      irritability: { median: 2, dayCount: 1 },
      concentration: { median: 4, dayCount: 1 },
      impulsivity: { median: 3, dayCount: 1 },
    });
    expect(JSON.stringify(model)).not.toMatch(BLOCKED_LANGUAGE);
  });

  it("uses a calm empty state before any local day exists", () => {
    const model = toDailyTimelineDashboardModel({
      type: "daily_timeline",
      protocol_version: PROTOCOL_VERSION,
      generated_at: "2026-06-14T12:00:00Z",
      summary: {
        calendar_day_count_before_limit: 0,
        returned_day_count: 0,
        browser_day_count: 0,
        os_day_count: 0,
        check_in_day_count: 0,
        annotation_day_count: 0,
        missing_day_count: 0,
        days_truncated: false,
      },
      days: [],
    });

    expect(model.state).toBe("empty");
    expect(toLatestLocalRecordModel(model)).toEqual({ state: "empty" });
    expect(JSON.stringify(model)).not.toMatch(BLOCKED_LANGUAGE);
  });
});
