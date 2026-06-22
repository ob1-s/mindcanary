import type { Metric } from "@mindcanary/protocol";

export const BUCKET_DURATION_MS = 15 * 60 * 1000;

export type ActivityState = "active" | "idle";

export interface ReducerState {
  bucketStartMs: number;
  lastEventMs: number;
  bucketEligible: boolean;
  timeZone: string;
  openTabIds: number[];
  activeTabByWindow: Record<string, number>;
  windowCount: number;
  openTabCountMin: number;
  openTabCountMax: number;
  openTabCountAreaMs: number;
  windowCountMax: number;
  openedCount: number;
  closedCount: number;
  switchCount: number;
  activity: ActivityState | null;
  activityEligible: boolean;
  activeMs: number;
  idleMs: number;
}

export type ReducerEvent =
  | { type: "tab_created"; atMs: number; tabId: number }
  | { type: "tab_removed"; atMs: number; tabId: number }
  | {
      type: "tab_activated";
      atMs: number;
      tabId: number;
      windowId: number;
    }
  | {
      type: "snapshot";
      atMs: number;
      tabIds: number[];
      windowCount: number;
    }
  | {
      type: "window_count_changed";
      atMs: number;
      windowCount: number;
    }
  | {
      type: "activity_changed";
      atMs: number;
      activity: ActivityState | null;
    };

export interface CompletedBucket {
  startMs: number;
  endMs: number;
  metrics: Metric[];
}

export interface Reduction {
  state: ReducerState;
  completed: CompletedBucket[];
}

export interface BucketProgress {
  startMs: number;
  endMs: number;
  percent: number;
}

interface InitialState {
  nowMs: number;
  tabIds: number[];
  windowCount: number;
  activity?: ActivityState | null;
  timeZone?: string;
}

export function createReducerState({
  nowMs,
  tabIds,
  windowCount,
  activity = null,
  timeZone = "UTC",
}: InitialState): ReducerState {
  const uniqueTabIds = sortedUnique(tabIds);
  const bucketStartMs = floorToBucket(nowMs);

  return {
    bucketStartMs,
    lastEventMs: nowMs,
    bucketEligible: nowMs === bucketStartMs,
    timeZone,
    openTabIds: uniqueTabIds,
    activeTabByWindow: {},
    windowCount,
    openTabCountMin: uniqueTabIds.length,
    openTabCountMax: uniqueTabIds.length,
    openTabCountAreaMs: 0,
    windowCountMax: windowCount,
    openedCount: 0,
    closedCount: 0,
    switchCount: 0,
    activity,
    activityEligible: activity !== null && nowMs === bucketStartMs,
    activeMs: 0,
    idleMs: 0,
  };
}

export function applyEvent(
  inputState: ReducerState,
  event: ReducerEvent,
): Reduction {
  const reduction = advanceTo(inputState, event.atMs);
  const state = reduction.state;

  switch (event.type) {
    case "tab_created":
      if (!state.openTabIds.includes(event.tabId)) {
        state.openTabIds = sortedUnique([...state.openTabIds, event.tabId]);
        state.openedCount += 1;
        updateTabExtrema(state);
      }
      break;
    case "tab_removed":
      if (state.openTabIds.includes(event.tabId)) {
        state.openTabIds = state.openTabIds.filter(
          (tabId) => tabId !== event.tabId,
        );
        state.closedCount += 1;
        updateTabExtrema(state);
      }
      removeActiveTab(state, event.tabId);
      break;
    case "tab_activated": {
      const windowKey = String(event.windowId);
      const previousTabId = state.activeTabByWindow[windowKey];
      if (previousTabId !== undefined && previousTabId !== event.tabId) {
        state.switchCount += 1;
      }
      state.activeTabByWindow[windowKey] = event.tabId;
      break;
    }
    case "snapshot":
      state.openTabIds = sortedUnique(event.tabIds);
      state.windowCount = nonNegativeInteger(event.windowCount);
      state.windowCountMax = Math.max(state.windowCountMax, state.windowCount);
      state.activeTabByWindow = {};
      updateTabExtrema(state);
      break;
    case "window_count_changed":
      state.windowCount = nonNegativeInteger(event.windowCount);
      state.windowCountMax = Math.max(state.windowCountMax, state.windowCount);
      break;
    case "activity_changed":
      if (event.activity === null || state.activity === null) {
        state.activityEligible = false;
      }
      state.activity = event.activity;
      break;
  }

  return reduction;
}

export function flushThrough(
  state: ReducerState,
  throughMs: number,
): Reduction {
  return advanceTo(state, throughMs);
}

export function currentBucketProgress(
  state: ReducerState,
  nowMs: number,
): BucketProgress {
  const endMs = state.bucketStartMs + BUCKET_DURATION_MS;
  const elapsedMs = Math.min(
    BUCKET_DURATION_MS,
    Math.max(0, nowMs - state.bucketStartMs),
  );

  return {
    startMs: state.bucketStartMs,
    endMs,
    percent: Math.floor((elapsedMs / BUCKET_DURATION_MS) * 100),
  };
}

function advanceTo(inputState: ReducerState, targetMs: number): Reduction {
  if (!Number.isSafeInteger(targetMs) || targetMs < inputState.lastEventMs) {
    throw new Error(
      "reducer events must have monotonic millisecond timestamps",
    );
  }

  let state = cloneState(inputState);
  const completed: CompletedBucket[] = [];

  while (targetMs >= state.bucketStartMs + BUCKET_DURATION_MS) {
    const bucketEndMs = state.bucketStartMs + BUCKET_DURATION_MS;
    accrue(state, bucketEndMs);
    if (state.bucketEligible) {
      completed.push(completeBucket(state, bucketEndMs));
    }
    state = nextBucketState(state, bucketEndMs);
  }

  accrue(state, targetMs);
  return { state, completed };
}

function accrue(state: ReducerState, targetMs: number): void {
  const elapsedMs = targetMs - state.lastEventMs;
  if (elapsedMs <= 0) {
    return;
  }

  state.openTabCountAreaMs += state.openTabIds.length * elapsedMs;
  if (state.activity === "active") {
    state.activeMs += elapsedMs;
  } else if (state.activity === "idle") {
    state.idleMs += elapsedMs;
  }
  state.lastEventMs = targetMs;
}

function completeBucket(
  state: ReducerState,
  bucketEndMs: number,
): CompletedBucket {
  const meanTabCount = state.openTabCountAreaMs / BUCKET_DURATION_MS;
  const metrics: Metric[] = [
    {
      signal: "browser.open_tab_count_min",
      value: state.openTabCountMin,
    },
    {
      signal: "browser.open_tab_count_max",
      value: state.openTabCountMax,
    },
    {
      signal: "browser.open_tab_count_mean",
      value: meanTabCount,
    },
    { signal: "browser.tab_open_count", value: state.openedCount },
    { signal: "browser.tab_close_count", value: state.closedCount },
    { signal: "browser.tab_switch_count", value: state.switchCount },
    { signal: "browser.window_count_max", value: state.windowCountMax },
  ];

  if (state.activityEligible) {
    metrics.push({
      signal: "browser.active_seconds",
      value: state.activeMs / 1000,
    });
    metrics.push({
      signal: "browser.idle_seconds",
      value: state.idleMs / 1000,
    });
  }

  return {
    startMs: state.bucketStartMs,
    endMs: bucketEndMs,
    metrics,
  };
}

function nextBucketState(
  previous: ReducerState,
  bucketStartMs: number,
): ReducerState {
  const openTabCount = previous.openTabIds.length;

  return {
    bucketStartMs,
    lastEventMs: bucketStartMs,
    bucketEligible: true,
    timeZone: previous.timeZone,
    openTabIds: [...previous.openTabIds],
    activeTabByWindow: { ...previous.activeTabByWindow },
    windowCount: previous.windowCount,
    openTabCountMin: openTabCount,
    openTabCountMax: openTabCount,
    openTabCountAreaMs: 0,
    windowCountMax: previous.windowCount,
    openedCount: 0,
    closedCount: 0,
    switchCount: 0,
    activity: previous.activity,
    activityEligible: previous.activity !== null,
    activeMs: 0,
    idleMs: 0,
  };
}

function cloneState(state: ReducerState): ReducerState {
  return {
    ...state,
    bucketEligible: state.bucketEligible ?? false,
    timeZone: state.timeZone ?? "UTC",
    activityEligible: state.activityEligible ?? false,
    openTabIds: [...state.openTabIds],
    activeTabByWindow: { ...state.activeTabByWindow },
  };
}

function updateTabExtrema(state: ReducerState): void {
  state.openTabCountMin = Math.min(
    state.openTabCountMin,
    state.openTabIds.length,
  );
  state.openTabCountMax = Math.max(
    state.openTabCountMax,
    state.openTabIds.length,
  );
}

function removeActiveTab(state: ReducerState, tabId: number): void {
  state.activeTabByWindow = Object.fromEntries(
    Object.entries(state.activeTabByWindow).filter(
      ([, activeTabId]) => activeTabId !== tabId,
    ),
  );
}

function sortedUnique(values: number[]): number[] {
  return [...new Set(values.map(nonNegativeInteger))].sort(
    (left, right) => left - right,
  );
}

function nonNegativeInteger(value: number): number {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("collector identifiers and counts must be non-negative");
  }
  return value;
}

function floorToBucket(timestampMs: number): number {
  return Math.floor(timestampMs / BUCKET_DURATION_MS) * BUCKET_DURATION_MS;
}
