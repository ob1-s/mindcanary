import { describe, expect, it } from "vitest";

import {
  PROTOCOL_VERSION,
  SIGNAL_IDS,
  type ProtocolResponse,
} from "@mindcanary/protocol";

import {
  BROWSER_STARTER_SIGNALS,
  OS_ACTIVITY_SIGNALS,
  browserCollectionControls,
  createCollectionSettingsRequest,
  createDeleteSignalRecordsRequest,
  createPrepareDeleteSignalRecordsRequest,
  createSetSignalCollectionRequest,
  enableBrowserStarterSet,
  enableOsActivityStarterSet,
  osActivityCollectionControls,
  osCollectionControls,
  toBrowserStarterSetModel,
  toOsActivityStarterSetModel,
  toSignalDeletionConfirmation,
  toSignalDeletionResult,
  toSignalCollectionControls,
} from "./collection-controls";

describe("collection controls", () => {
  it("builds typed local setting requests", () => {
    expect(createCollectionSettingsRequest()).toEqual({
      type: "get_collection_settings",
      protocol_version: PROTOCOL_VERSION,
    });
    expect(
      createSetSignalCollectionRequest("browser.tab_switch_count", true),
    ).toEqual({
      type: "set_signal_collection",
      protocol_version: PROTOCOL_VERSION,
      signal: "browser.tab_switch_count",
      enabled: true,
    });
    expect(
      createPrepareDeleteSignalRecordsRequest("browser.tab_switch_count"),
    ).toEqual({
      type: "prepare_delete_signal_records",
      protocol_version: PROTOCOL_VERSION,
      signal: "browser.tab_switch_count",
    });
    expect(
      createDeleteSignalRecordsRequest(
        "browser.tab_switch_count",
        "confirmation-token",
      ),
    ).toEqual({
      type: "delete_signal_records",
      protocol_version: PROTOCOL_VERSION,
      signal: "browser.tab_switch_count",
      confirmation_token: "confirmation-token",
    });
    expect(() =>
      createDeleteSignalRecordsRequest("browser.tab_switch_count", ""),
    ).toThrow(TypeError);
  });

  it("shows every signal as disabled until explicitly enabled", () => {
    const response: ProtocolResponse = {
      type: "collection_settings",
      protocol_version: PROTOCOL_VERSION,
      settings: [],
    };

    const controls = toSignalCollectionControls(response);

    expect(controls).toHaveLength(SIGNAL_IDS.length);
    expect(controls.every((control) => !control.enabled)).toBe(true);
    expect(
      controls.every((control) => /Never enabled/.test(control.statusText)),
    ).toBe(true);
    expect(JSON.stringify(controls)).not.toMatch(
      /url|title|history|page text/i,
    );
  });

  it("renders the local transition time without clinical framing", () => {
    const response: ProtocolResponse = {
      type: "collection_settings",
      protocol_version: PROTOCOL_VERSION,
      settings: [
        {
          signal: "browser.open_tab_count_mean",
          enabled: false,
          changed_at: "2026-06-14T12:00:00Z",
        },
      ],
    };

    const control = toSignalCollectionControls(response).find(
      ({ signal }) => signal === "browser.open_tab_count_mean",
    );

    expect(control?.statusText).toBe("Paused locally at 2026-06-14T12:00:00Z");
    expect(JSON.stringify(control)).not.toMatch(
      /mania|depression|psychosis|warning|risk/i,
    );
  });

  it("keeps the browser panel and starter set limited to browser aggregates", () => {
    const controls = toSignalCollectionControls({
      type: "collection_settings",
      protocol_version: PROTOCOL_VERSION,
      settings: [
        {
          signal: "browser.open_tab_count_mean",
          enabled: true,
          changed_at: "2026-06-14T12:00:00Z",
        },
        {
          signal: "os.active_seconds",
          enabled: true,
          changed_at: "2026-06-14T12:00:00Z",
        },
      ],
    });
    const browserControls = browserCollectionControls(controls);
    const starter = toBrowserStarterSetModel(browserControls);

    expect(browserControls.length).toBeGreaterThan(0);
    expect(
      browserControls.every((control) => control.signal.startsWith("browser.")),
    ).toBe(true);
    expect(starter.enabledCount).toBe(1);
    expect(starter.fullyEnabled).toBe(false);
    expect(BROWSER_STARTER_SIGNALS).toEqual([
      "browser.tab_switch_count",
      "browser.open_tab_count_max",
      "browser.open_tab_count_mean",
      "browser.window_count_max",
      "browser.active_seconds",
      "browser.idle_seconds",
      "browser.retained_across_day_count",
    ]);
    expect(JSON.stringify(starter)).not.toMatch(/url|title|history|page text/i);
  });

  it("keeps the OS activity panel limited to active and idle duration aggregates", () => {
    const controls = toSignalCollectionControls({
      type: "collection_settings",
      protocol_version: PROTOCOL_VERSION,
      settings: [
        {
          signal: "browser.open_tab_count_mean",
          enabled: true,
          changed_at: "2026-06-14T12:00:00Z",
        },
        {
          signal: "os.active_seconds",
          enabled: true,
          changed_at: "2026-06-14T12:00:00Z",
        },
        {
          signal: "os.lock_count",
          enabled: true,
          changed_at: "2026-06-14T12:00:00Z",
        },
      ],
    });
    const osControls = osActivityCollectionControls(controls);
    const starter = toOsActivityStarterSetModel(osControls);

    expect(osControls.map((control) => control.signal)).toEqual(
      OS_ACTIVITY_SIGNALS,
    );
    expect(OS_ACTIVITY_SIGNALS).toEqual([
      "os.active_seconds",
      "os.idle_seconds",
    ]);
    expect(
      osControls.some((control) => control.signal === "os.lock_count"),
    ).toBe(false);
    expect(starter).toMatchObject({
      enabledCount: 1,
      totalCount: 2,
      fullyEnabled: false,
    });
    expect(JSON.stringify(osControls)).not.toMatch(
      /url|title|history|page text|window title|app name/i,
    );
  });

  it("keeps lifecycle signals optional beside the activity starter set", () => {
    const controls = toSignalCollectionControls({
      type: "collection_settings",
      protocol_version: PROTOCOL_VERSION,
      settings: [],
    });
    expect(
      osCollectionControls(controls).map((control) => control.signal),
    ).toEqual([
      "os.active_seconds",
      "os.idle_seconds",
      "os.lock_count",
      "os.unlock_count",
      "os.suspend_count",
      "os.resume_count",
    ]);
    expect(toOsActivityStarterSetModel(controls).signals).toEqual([
      "os.active_seconds",
      "os.idle_seconds",
    ]);
  });

  it("enables only missing starter signals and reports partial failures", async () => {
    const controls = toSignalCollectionControls({
      type: "collection_settings",
      protocol_version: PROTOCOL_VERSION,
      settings: [
        {
          signal: "browser.open_tab_count_mean",
          enabled: true,
          changed_at: "2026-06-14T12:00:00Z",
        },
      ],
    });
    const attempted: string[] = [];

    const result = await enableBrowserStarterSet(controls, async (signal) => {
      attempted.push(signal);
      if (signal === "browser.active_seconds") {
        throw new Error("local service unavailable");
      }
    });

    expect(attempted).not.toContain("browser.open_tab_count_mean");
    expect(result).toEqual({
      attemptedCount: 6,
      enabledCount: 5,
      failedSignals: ["browser.active_seconds"],
    });
  });

  it("enables only missing OS activity starter signals", async () => {
    const controls = toSignalCollectionControls({
      type: "collection_settings",
      protocol_version: PROTOCOL_VERSION,
      settings: [
        {
          signal: "os.active_seconds",
          enabled: true,
          changed_at: "2026-06-14T12:00:00Z",
        },
      ],
    });
    const attempted: string[] = [];

    const result = await enableOsActivityStarterSet(
      controls,
      async (signal) => {
        attempted.push(signal);
      },
    );

    expect(attempted).toEqual(["os.idle_seconds"]);
    expect(result).toEqual({
      attemptedCount: 1,
      enabledCount: 1,
      failedSignals: [],
    });
  });

  it("states the exact scope of signal deletion", () => {
    const confirmation = toSignalDeletionConfirmation({
      type: "delete_signal_records_confirmation",
      protocol_version: PROTOCOL_VERSION,
      confirmation_token: "01900000-0000-7000-8000-000000000001",
      expires_at: "2026-06-14T12:05:00Z",
      signal: "browser.open_tab_count_mean",
      summary: {
        metric_record_count: 12,
        affected_batch_count: 10,
      },
    });

    expect(confirmation.summaryText).toBe(
      "12 stored values across 10 aggregate periods",
    );
    expect(confirmation.confirmationText).toMatch(
      /only stored average open tabs/i,
    );
    expect(confirmation.confirmationText).toMatch(/does not delete check-ins/i);
    expect(confirmation.confirmationText).toMatch(
      /does not change future collection/i,
    );

    const result = toSignalDeletionResult({
      type: "signal_records_deleted",
      protocol_version: PROTOCOL_VERSION,
      signal: "browser.open_tab_count_mean",
      deleted: {
        metric_record_count: 12,
        affected_batch_count: 10,
      },
    });
    expect(result.label).toBe("Average open tabs");
    expect(JSON.stringify({ confirmation, result })).not.toMatch(
      /mania|depression|psychosis|warning|risk/i,
    );
  });
});
