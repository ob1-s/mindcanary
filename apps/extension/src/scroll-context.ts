export const CONTINUOUS_SCROLLING_SIGNAL =
  "browser.continuous_scrolling_seconds" as const;

const BUCKET_MS = 15 * 60 * 1000;
const MAX_ATTRIBUTABLE_GAP_MS = 90 * 1000;

export interface ScrollContextState {
  bucketStartMs: number;
  lastEventMs: number;
  bucketEligible: boolean;
  activeTabIds: number[];
  activeMs: number;
}

export interface ScrollContextEvent {
  atMs: number;
  tabId: number;
  active: boolean;
}

export interface CompletedScrollBucket {
  startMs: number;
  endMs: number;
  seconds: number;
}

export interface ScrollContextReduction {
  state: ScrollContextState;
  completed: CompletedScrollBucket[];
}

export function createScrollContextState(nowMs: number): ScrollContextState {
  validateTimestamp(nowMs);
  const bucketStartMs = alignedBucketStart(nowMs);
  return {
    bucketStartMs,
    lastEventMs: nowMs,
    bucketEligible: nowMs === bucketStartMs,
    activeTabIds: [],
    activeMs: 0,
  };
}

export function applyScrollContextEvent(
  state: ScrollContextState,
  event: ScrollContextEvent,
): ScrollContextReduction {
  validateTabId(event.tabId);
  const reduction = advanceScrollContext(state, event.atMs);
  const activeTabIds = new Set(reduction.state.activeTabIds);
  if (event.active) {
    activeTabIds.add(event.tabId);
  } else {
    activeTabIds.delete(event.tabId);
  }
  reduction.state.activeTabIds = [...activeTabIds].sort(
    (left, right) => left - right,
  );
  return reduction;
}

export function advanceScrollContext(
  state: ScrollContextState,
  nowMs: number,
): ScrollContextReduction {
  validateTimestamp(nowMs);
  if (nowMs < state.lastEventMs) {
    throw new RangeError("scroll context events must be monotonic");
  }
  if (nowMs - state.lastEventMs > MAX_ATTRIBUTABLE_GAP_MS) {
    return { state: createScrollContextState(nowMs), completed: [] };
  }

  const current = cloneState(state);
  const completed: CompletedScrollBucket[] = [];
  while (current.lastEventMs < nowMs) {
    const bucketEndMs = current.bucketStartMs + BUCKET_MS;
    const segmentEndMs = Math.min(nowMs, bucketEndMs);
    if (current.activeTabIds.length > 0) {
      current.activeMs += segmentEndMs - current.lastEventMs;
    }
    current.lastEventMs = segmentEndMs;
    if (current.lastEventMs === bucketEndMs) {
      if (current.bucketEligible) {
        completed.push({
          startMs: current.bucketStartMs,
          endMs: bucketEndMs,
          seconds: current.activeMs / 1000,
        });
      }
      current.bucketStartMs = bucketEndMs;
      current.activeMs = 0;
      current.bucketEligible = true;
    }
  }
  return { state: current, completed };
}

function cloneState(state: ScrollContextState): ScrollContextState {
  return { ...state, activeTabIds: [...state.activeTabIds] };
}

function alignedBucketStart(nowMs: number): number {
  return Math.floor(nowMs / BUCKET_MS) * BUCKET_MS;
}

function validateTimestamp(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RangeError(
      "scroll context timestamps must be non-negative integers",
    );
  }
}

function validateTabId(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RangeError(
      "scroll context tab IDs must be non-negative integers",
    );
  }
}
