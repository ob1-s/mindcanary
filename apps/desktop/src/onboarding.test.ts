import { describe, expect, it } from "vitest";

import { ONBOARDING_STORAGE_KEY, toOnboardingModel } from "./onboarding";
import type { SetupChecklistModel } from "./setup";

const incompleteSetup: SetupChecklistModel = {
  title: "Start your private logbook",
  description: "Start with one check-in.",
  progressText: "1 of 2 essentials ready",
  complete: false,
  steps: [],
};

const completeSetup: SetupChecklistModel = {
  ...incompleteSetup,
  title: "Private logbook is ready",
  progressText: "2 of 2 essentials ready",
  complete: true,
};

describe("first-run onboarding", () => {
  it("uses a versioned local persistence key", () => {
    expect(ONBOARDING_STORAGE_KEY).toContain("onboarding.completed.v1");
  });

  it("shows only before local setup is complete or dismissed", () => {
    expect(
      toOnboardingModel({
        completedLocally: false,
        setup: incompleteSetup,
        serviceState: "ready",
      }).show,
    ).toBe(true);

    expect(
      toOnboardingModel({
        completedLocally: true,
        setup: incompleteSetup,
        serviceState: "ready",
      }).show,
    ).toBe(false);

    expect(
      toOnboardingModel({
        completedLocally: false,
        setup: completeSetup,
        serviceState: "ready",
      }).show,
    ).toBe(false);

    expect(
      toOnboardingModel({
        completedLocally: false,
        setup: incompleteSetup,
        serviceState: "checking",
      }).show,
    ).toBe(false);
  });

  it("keeps unavailable local service copy practical instead of alarming", () => {
    const model = toOnboardingModel({
      completedLocally: false,
      setup: incompleteSetup,
      serviceState: "unavailable",
    });

    expect(model.serviceText).toContain("needs attention");
    expect(JSON.stringify(model)).not.toMatch(/warning|diagnosis|risk/i);
  });
});
