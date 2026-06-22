import { describe, expect, it } from "vitest";

import { enabledSignalIds, filterEnabledMetrics } from "./collection-policy";

describe("collector signal policy", () => {
  it("defaults to no enabled signals", () => {
    expect(enabledSignalIds([])).toEqual([]);
  });

  it("retains only explicitly enabled aggregate metrics", () => {
    const enabled = new Set(
      enabledSignalIds([
        {
          signal: "browser.tab_switch_count",
          enabled: true,
          changed_at: "2026-06-14T12:00:00Z",
        },
        {
          signal: "browser.open_tab_count_mean",
          enabled: false,
          changed_at: "2026-06-14T12:05:00Z",
        },
      ]),
    );

    expect(
      filterEnabledMetrics(
        [
          { signal: "browser.tab_switch_count", value: 12 },
          { signal: "browser.open_tab_count_mean", value: 30 },
        ],
        enabled,
      ),
    ).toEqual([{ signal: "browser.tab_switch_count", value: 12 }]);
  });
});
