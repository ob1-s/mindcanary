import type { LocalServiceState, SetupChecklistModel } from "./setup";

export const ONBOARDING_STORAGE_KEY =
  "mindcanary.desktop.onboarding.completed.v1";

export type OnboardingStep = "intro" | "startup" | "browser" | "computer";

export interface OnboardingModel {
  show: boolean;
  serviceText: string;
  primaryActionLabel: string;
}

export function toOnboardingModel(input: {
  completedLocally: boolean;
  setup: SetupChecklistModel;
  serviceState: LocalServiceState;
}): OnboardingModel {
  const show =
    !input.completedLocally &&
    !input.setup.complete &&
    input.serviceState !== "checking";

  return {
    show,
    serviceText: serviceText(input.serviceState),
    primaryActionLabel:
      input.serviceState === "ready" ? "Get started" : "Continue",
  };
}

function serviceText(serviceState: LocalServiceState): string {
  switch (serviceState) {
    case "checking":
      return "Checking the local service...";
    case "ready":
      return "Local service is ready.";
    case "unavailable":
      return "The local service needs attention before records can be saved.";
  }
}
