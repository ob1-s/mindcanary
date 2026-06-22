import { describe, expect, it } from "vitest";

import { CONTEXT_TAGS, PROTOCOL_VERSION, SIGNAL_IDS } from "./generated";
import type { ProtocolRequest, ProtocolResponse } from "./generated";

describe("generated collector protocol", () => {
  it("exports the supported protocol version", () => {
    expect(PROTOCOL_VERSION).toBe(1);
  });

  it("contains only aggregate signal names", () => {
    expect(SIGNAL_IDS).not.toHaveLength(0);
    expect(SIGNAL_IDS.join(" ")).not.toMatch(
      /url|title|query|text|content|keystroke|clipboard/i,
    );
  });

  it("does not expose clinical labels in context tags", () => {
    expect(CONTEXT_TAGS).not.toHaveLength(0);
    expect(CONTEXT_TAGS.join(" ")).not.toMatch(
      /mania|depression|psychosis|diagnosis|warning/i,
    );
  });

  it("supports typed daily rhythm insight read messages", () => {
    const request: ProtocolRequest = {
      type: "get_daily_rhythm_insights",
      protocol_version: PROTOCOL_VERSION,
      limit: 10,
    };
    const response: ProtocolResponse = {
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
      ],
      insights: [
        {
          local_date: "2026-01-08",
          dimension: "browser_tabs",
          direction: "higher",
          summary:
            "Open-tab average was higher than your previous baseline on 2026-01-08.",
          evidence: [
            { label: "current", value: "24 tabs" },
            { label: "baseline median", value: "9.5 tabs" },
          ],
        },
      ],
    };

    expect(request.type).toBe("get_daily_rhythm_insights");
    expect(response.insights[0]?.dimension).toBe("browser_tabs");
    expect(response.readiness[0]?.status).toBe("change_described");
    expect(JSON.stringify(response)).not.toMatch(
      /mania|depression|psychosis|diagnosis|warning/i,
    );
  });

  it("supports explicit per-signal collection settings", () => {
    const request: ProtocolRequest = {
      type: "set_signal_collection",
      protocol_version: PROTOCOL_VERSION,
      signal: "browser.tab_switch_count",
      enabled: false,
    };
    const response: ProtocolResponse = {
      type: "collection_settings",
      protocol_version: PROTOCOL_VERSION,
      settings: [
        {
          signal: "browser.tab_switch_count",
          enabled: false,
          changed_at: "2026-06-14T12:00:00Z",
        },
      ],
    };

    expect(request.enabled).toBe(false);
    expect(response.settings[0]?.enabled).toBe(false);
  });
});
