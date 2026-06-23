import { describe, expect, it } from "vitest";

import { toCollectorStatusViewModel, type CollectorStatus } from "./status";

function status(overrides: Partial<CollectorStatus> = {}): CollectorStatus {
  return {
    browserTarget: "chrome",
    buildChannel: "development",
    extensionId: "abcdefghijklmnopabcdefghijklmnop",
    expectedExtensionId: "abcdefghijklmnopabcdefghijklmnop",
    identityMatches: true,
    nativeHostName: "app.mindcanary.collector",
    enabledSignals: [],
    idlePermissionGranted: false,
    scrollPermissionGranted: false,
    pendingBatchCount: 0,
    droppedBatchCount: 0,
    reducerActive: false,
    nextSequence: 0,
    settingsStatus: {
      state: "ok",
      checkedAt: "2026-06-14T12:00:00Z",
    },
    ...overrides,
  };
}

describe("collector status view model", () => {
  it("explains when the native host is not connected", () => {
    const model = toCollectorStatusViewModel(
      status({
        settingsStatus: {
          state: "unavailable",
          checkedAt: "2026-06-14T12:00:00Z",
        },
      }),
    );

    expect(model.state).toBe("needs_setup");
    expect(model.headline).toMatch(/Native host/);
    expect(model.nextActionText).toContain("native-host setup command");
    expect(model.extensionIdText).toBe("abcdefghijklmnopabcdefghijklmnop");
    expect(model.nativeHostText).toBe("app.mindcanary.collector");
    expect(model.setupCommand).toContain("--channel development");
    expect(model.setupCommand).not.toContain("--extension-id");
    expect(JSON.stringify(model)).not.toMatch(/diagnosis|warning|mania/i);
  });

  it("fails visibly when the runtime and build identities differ", () => {
    const model = toCollectorStatusViewModel(
      status({
        identityMatches: false,
        expectedExtensionId: "agokdhalkipifklmbipkgmfakdcaekbj",
      }),
    );

    expect(model.state).toBe("needs_setup");
    expect(model.headline).toBe("Extension identity mismatch");
    expect(model.setupCommand).toBeNull();
  });

  it("explains default-off collection without implying a failure", () => {
    const model = toCollectorStatusViewModel(status());

    expect(model.state).toBe("disabled");
    expect(model.detail).toContain("Enable");
    expect(model.nextActionText).toContain("desktop app");
    expect(model.enabledSignalText).toBe("0 enabled signals");
    expect(model.idlePermissionText).toBe(
      "Idle permission not needed for enabled signals",
    );
    expect(model.showIdlePermissionRequest).toBe(false);
    expect(model.setupCommand).toBeNull();
  });

  it("asks for idle permission only when active or idle aggregates need it", () => {
    const model = toCollectorStatusViewModel(
      status({
        enabledSignals: ["browser.active_seconds", "browser.idle_seconds"],
      }),
    );

    expect(model.idlePermissionText).toBe(
      "Idle permission needed for browser active/idle aggregates",
    );
    expect(model.nextActionText).toContain("Allow idle permission");
    expect(model.showIdlePermissionRequest).toBe(true);
  });

  it("hides the idle permission request after permission is granted", () => {
    const model = toCollectorStatusViewModel(
      status({
        enabledSignals: ["browser.active_seconds"],
        idlePermissionGranted: true,
      }),
    );

    expect(model.idlePermissionText).toBe("Idle permission granted");
    expect(model.showIdlePermissionRequest).toBe(false);
  });

  it("requests only the narrow feed permission when its signal is enabled", () => {
    const model = toCollectorStatusViewModel(
      status({
        enabledSignals: ["browser.continuous_scrolling_seconds"],
      }),
    );
    expect(model.showScrollPermissionRequest).toBe(true);
    expect(model.scrollPermissionText).toContain("x.com");
    expect(model.nextActionText).toContain("selected feed-site permission");
    expect(JSON.stringify(model)).not.toMatch(/doom|waste|shame|score/i);
  });

  it("shows queued aggregate batches without exposing browsing content", () => {
    const model = toCollectorStatusViewModel(
      status({
        enabledSignals: ["browser.open_tab_count_mean"],
        pendingBatchCount: 3,
      }),
    );

    expect(model.state).toBe("queued");
    expect(model.nextActionText).toContain("local service");
    expect(model.pendingBatchText).toBe("3 queued batches");
    expect(model.showQueueReset).toBe(true);
    expect(model.queueResetText).toContain("unsent aggregate queue");
    expect(model.queueResetText).toContain("Delivered local records");
    expect(JSON.stringify(model)).not.toMatch(/url|title|history|page text/i);
  });

  it("surfaces dropped batches after the retry queue limit is reached", () => {
    const model = toCollectorStatusViewModel(
      status({
        enabledSignals: ["browser.open_tab_count_mean"],
        pendingBatchCount: 96,
        droppedBatchCount: 4,
      }),
    );

    expect(model.pendingBatchText).toBe(
      "96 queued batches; 4 dropped after queue limit",
    );
    expect(model.showQueueReset).toBe(true);
    expect(JSON.stringify(model)).not.toMatch(/url|title|history|page text/i);
  });

  it("hides queue reset when there is no local retry state", () => {
    const model = toCollectorStatusViewModel(status());

    expect(model.showQueueReset).toBe(false);
  });

  it("shows active aggregate collection", () => {
    const model = toCollectorStatusViewModel(
      status({
        enabledSignals: ["browser.open_tab_count_mean"],
        reducerActive: true,
        activeBucket: {
          startedAt: "2026-06-14T12:00:00Z",
          endsAt: "2026-06-14T12:15:00Z",
          progressPercent: 40,
        },
      }),
    );

    expect(model.state).toBe("collecting");
    expect(model.nextActionText).toContain("15-minute buckets");
    expect(model.detail).toContain("No URLs");
    expect(model.bucketProgressPercent).toBe(40);
    expect(model.bucketProgressText).toContain("40% complete");
    expect(
      JSON.stringify({
        percent: model.bucketProgressPercent,
        text: model.bucketProgressText,
      }),
    ).not.toMatch(/url|title|domain|search term|page text/i);
  });
});
