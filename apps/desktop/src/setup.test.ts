import { describe, expect, it } from "vitest";

import type { SignalCollectionControlModel } from "./collection-controls";
import { toSetupChecklistModel } from "./setup";

const browserControl: SignalCollectionControlModel = {
  signal: "browser.open_tab_count_mean",
  label: "Average open tabs",
  description: "Records an aggregate count.",
  enabled: false,
  statusText: "Never enabled on this device",
};

describe("local setup checklist", () => {
  it("makes the local service the first actionable step when unavailable", () => {
    const model = toSetupChecklistModel({
      serviceState: "unavailable",
      collection: [],
      collectionUnavailable: true,
      hasLocalRecords: undefined,
      localDataUnavailable: true,
    });

    expect(model.progressText).toBe("0 of 2 essentials ready");
    expect(model.steps[0]?.state).toBe("current");
    expect(model.steps[0]?.detail).toContain("local service");
    expect(model.steps[1]?.state).toBe("blocked");
    expect(model.steps[2]?.state).toBe("blocked");
  });

  it("guides signal selection and the first local record independently", () => {
    const model = toSetupChecklistModel({
      serviceState: "ready",
      collection: [browserControl],
      collectionUnavailable: false,
      hasLocalRecords: false,
      localDataUnavailable: false,
    });

    expect(model.progressText).toBe("1 of 2 essentials ready");
    expect(model.steps[1]).toMatchObject({
      state: "current",
      actionTarget: "#check-in-panel",
    });
    expect(model.steps[2]).toMatchObject({
      state: "optional",
      required: false,
      actionTarget: "#browser-signals-panel",
    });
  });

  it("reports completion after a local record without requiring browser signals", () => {
    const model = toSetupChecklistModel({
      serviceState: "ready",
      collection: [browserControl],
      collectionUnavailable: false,
      hasLocalRecords: true,
      localDataUnavailable: false,
    });

    expect(model.complete).toBe(true);
    expect(model.title).toBe("Private logbook is ready");
    expect(model.progressText).toBe("2 of 2 essentials ready");
    expect(model.steps[0]?.state).toBe("complete");
    expect(model.steps[1]?.state).toBe("complete");
    expect(model.steps[2]).toMatchObject({
      required: false,
      state: "optional",
      statusLabel: "Optional",
    });
    expect(JSON.stringify(model)).not.toMatch(/diagnosis|mania|warning/i);
  });
});
