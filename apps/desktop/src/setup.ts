import type { SignalCollectionControlModel } from "./collection-controls";

export type LocalServiceState = "checking" | "ready" | "unavailable";
export type SetupStepState =
  | "checking"
  | "complete"
  | "current"
  | "blocked"
  | "optional";

export interface SetupStepModel {
  id: "service" | "browser-signals" | "first-record";
  title: string;
  detail: string;
  state: SetupStepState;
  required: boolean;
  statusLabel: string;
  actionLabel?: string;
  actionTarget?: string;
}

export interface SetupChecklistModel {
  title: string;
  description: string;
  progressText: string;
  complete: boolean;
  steps: SetupStepModel[];
}

export interface SetupChecklistInput {
  serviceState: LocalServiceState;
  collection: SignalCollectionControlModel[];
  collectionUnavailable: boolean;
  hasLocalRecords?: boolean;
  localDataUnavailable: boolean;
}

export function toSetupChecklistModel(
  input: SetupChecklistInput,
): SetupChecklistModel {
  const enabledBrowserSignals = input.collection.filter(
    (control) =>
      control.signal.startsWith("browser.") && control.enabled === true,
  );
  const steps = [
    serviceStep(input.serviceState),
    firstRecordStep(input),
    browserSignalsStep(input, enabledBrowserSignals.length),
  ];
  const requiredSteps = steps.filter((step) => step.required);
  const completedRequiredCount = requiredSteps.filter(
    (step) => step.state === "complete",
  ).length;
  const complete = completedRequiredCount === requiredSteps.length;

  return {
    title: complete ? "Private logbook is ready" : "Start your private logbook",
    description: complete
      ? "mindcanary can keep a local rhythm history. Optional connectors can add context whenever you choose."
      : "Start with one check-in or aggregate record. Browser signals are optional context, not a requirement.",
    progressText: `${completedRequiredCount} of ${requiredSteps.length} essentials ready`,
    complete,
    steps,
  };
}

function serviceStep(serviceState: LocalServiceState): SetupStepModel {
  switch (serviceState) {
    case "checking":
      return {
        id: "service",
        title: "Local service",
        detail: "Checking the private local daemon.",
        state: "checking",
        required: true,
        statusLabel: "Checking",
      };
    case "ready":
      return {
        id: "service",
        title: "Local service",
        detail: "The local service is running on this device.",
        state: "complete",
        required: true,
        statusLabel: "Ready",
      };
    case "unavailable":
      return {
        id: "service",
        title: "Local service",
        detail: "Start the local service, then refresh or reopen the app.",
        state: "current",
        required: true,
        statusLabel: "Needs attention",
      };
  }
}

function browserSignalsStep(
  input: SetupChecklistInput,
  enabledCount: number,
): SetupStepModel {
  if (input.serviceState === "checking") {
    return {
      id: "browser-signals",
      title: "Browser aggregates",
      detail: "Waiting for the local service check.",
      state: "checking",
      required: false,
      statusLabel: "Waiting",
    };
  }
  if (input.serviceState === "unavailable") {
    return {
      id: "browser-signals",
      title: "Browser aggregates",
      detail: "The local service must be ready before settings can be loaded.",
      state: "blocked",
      required: false,
      statusLabel: "Waiting",
    };
  }
  if (input.collectionUnavailable || input.collection.length === 0) {
    return {
      id: "browser-signals",
      title: "Browser aggregates",
      detail: "Reading the local collection settings.",
      state: "checking",
      required: false,
      statusLabel: "Checking",
    };
  }
  if (enabledCount > 0) {
    return {
      id: "browser-signals",
      title: "Browser aggregates",
      detail: `${enabledCount} browser ${enabledCount === 1 ? "signal is" : "signals are"} enabled.`,
      state: "complete",
      required: false,
      statusLabel: "Ready",
    };
  }
  return {
    id: "browser-signals",
    title: "Browser aggregates",
    detail: "Optional: add aggregate browser context when you want it.",
    state: "optional",
    required: false,
    statusLabel: "Optional",
    actionLabel: "Choose browser signals",
    actionTarget: "#browser-signals-panel",
  };
}

function firstRecordStep(input: SetupChecklistInput): SetupStepModel {
  if (input.serviceState !== "ready") {
    return {
      id: "first-record",
      title: "First local record",
      detail: "The local service must be ready before records can be stored.",
      state: input.serviceState === "checking" ? "checking" : "blocked",
      required: true,
      statusLabel: "Waiting",
    };
  }
  if (input.localDataUnavailable || input.hasLocalRecords === undefined) {
    return {
      id: "first-record",
      title: "First local record",
      detail: "Reading the encrypted local record counts.",
      state: "checking",
      required: true,
      statusLabel: "Checking",
    };
  }
  if (input.hasLocalRecords) {
    return {
      id: "first-record",
      title: "First local record",
      detail: "At least one aggregate or optional check-in is stored locally.",
      state: "complete",
      required: true,
      statusLabel: "Ready",
    };
  }
  return {
    id: "first-record",
    title: "First local record",
    detail:
      "Save a check-in to start now. Optional connectors can add context later.",
    state: "current",
    required: true,
    statusLabel: "No records yet",
    actionLabel: "Add a check-in",
    actionTarget: "#check-in-panel",
  };
}
