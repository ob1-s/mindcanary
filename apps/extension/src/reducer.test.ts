import { describe, expect, it } from "vitest";

import {
  BUCKET_DURATION_MS,
  applyEvent,
  createReducerState,
  currentBucketProgress,
  flushThrough,
} from "./reducer";

const start = Date.UTC(2026, 0, 5, 12, 0, 0);

describe("tab rhythm reducer", () => {
  it("computes a time-weighted tab mean without URLs or titles", () => {
    let state = createReducerState({
      nowMs: start,
      tabIds: [1, 2],
      windowCount: 1,
    });

    state = applyEvent(state, {
      type: "tab_created",
      atMs: start + 5 * 60 * 1000,
      tabId: 3,
    }).state;

    const result = flushThrough(state, start + BUCKET_DURATION_MS);
    expect(result.completed).toHaveLength(1);
    expect(result.completed[0]?.metrics).toContainEqual({
      signal: "browser.open_tab_count_mean",
      value: 8 / 3,
    });
    expect(JSON.stringify(result.completed)).not.toMatch(
      /url|title|text|domain/i,
    );
  });

  it("counts switches only after a window has an active-tab baseline", () => {
    let state = createReducerState({
      nowMs: start,
      tabIds: [10, 11],
      windowCount: 1,
    });

    state = applyEvent(state, {
      type: "tab_activated",
      atMs: start + 1000,
      tabId: 10,
      windowId: 1,
    }).state;
    state = applyEvent(state, {
      type: "tab_activated",
      atMs: start + 2000,
      tabId: 11,
      windowId: 1,
    }).state;

    const result = flushThrough(state, start + BUCKET_DURATION_MS);
    expect(result.completed[0]?.metrics).toContainEqual({
      signal: "browser.tab_switch_count",
      value: 1,
    });
  });

  it("emits empty continuation buckets after long service-worker sleeps", () => {
    const state = createReducerState({
      nowMs: start,
      tabIds: [1],
      windowCount: 1,
      activity: "idle",
    });

    const result = flushThrough(state, start + 2 * BUCKET_DURATION_MS);
    expect(result.completed).toHaveLength(2);
    expect(result.completed[1]?.metrics).toContainEqual({
      signal: "browser.idle_seconds",
      value: 900,
    });
  });

  it("drops the partial first bucket after a reload", () => {
    const reloadedAt = start + 7 * 60 * 1000;
    const state = createReducerState({
      nowMs: reloadedAt,
      tabIds: [1, 2],
      windowCount: 1,
    });

    const firstBoundary = flushThrough(state, start + BUCKET_DURATION_MS);
    expect(firstBoundary.completed).toHaveLength(0);

    const nextBoundary = flushThrough(
      firstBoundary.state,
      start + 2 * BUCKET_DURATION_MS,
    );
    expect(nextBoundary.completed).toHaveLength(1);
    expect(nextBoundary.completed[0]?.metrics).toContainEqual({
      signal: "browser.open_tab_count_mean",
      value: 2,
    });
  });

  it("withholds active and idle duration without idle-state coverage", () => {
    const state = createReducerState({
      nowMs: start,
      tabIds: [1],
      windowCount: 1,
    });

    const result = flushThrough(state, start + BUCKET_DURATION_MS);
    const signals = result.completed[0]?.metrics.map((metric) => metric.signal);

    expect(signals).not.toContain("browser.active_seconds");
    expect(signals).not.toContain("browser.idle_seconds");
  });

  it("waits for a complete bucket after idle-state coverage begins", () => {
    let state = createReducerState({
      nowMs: start,
      tabIds: [1],
      windowCount: 1,
    });
    state = applyEvent(state, {
      type: "activity_changed",
      atMs: start + 5 * 60 * 1000,
      activity: "active",
    }).state;

    const firstBoundary = flushThrough(state, start + BUCKET_DURATION_MS);
    expect(
      firstBoundary.completed[0]?.metrics.map((metric) => metric.signal),
    ).not.toContain("browser.active_seconds");

    const nextBoundary = flushThrough(
      firstBoundary.state,
      start + 2 * BUCKET_DURATION_MS,
    );
    expect(nextBoundary.completed[0]?.metrics).toContainEqual({
      signal: "browser.active_seconds",
      value: 900,
    });
  });

  it("rejects out-of-order events", () => {
    const state = createReducerState({
      nowMs: start,
      tabIds: [],
      windowCount: 0,
    });

    expect(() =>
      applyEvent(state, {
        type: "tab_created",
        atMs: start - 1,
        tabId: 1,
      }),
    ).toThrow(/monotonic/);
  });

  it("reconciles snapshots without inventing open or close events", () => {
    const state = createReducerState({
      nowMs: start,
      tabIds: [1],
      windowCount: 1,
    });

    const reconciled = applyEvent(state, {
      type: "snapshot",
      atMs: start + 1000,
      tabIds: [2, 3],
      windowCount: 2,
    }).state;
    const result = flushThrough(reconciled, start + BUCKET_DURATION_MS);

    expect(result.completed[0]?.metrics).toContainEqual({
      signal: "browser.tab_open_count",
      value: 0,
    });
    expect(result.completed[0]?.metrics).toContainEqual({
      signal: "browser.tab_close_count",
      value: 0,
    });
  });

  it("reports coarse progress within the current fixed bucket", () => {
    const state = createReducerState({
      nowMs: start,
      tabIds: [1],
      windowCount: 1,
    });

    expect(currentBucketProgress(state, start - 1).percent).toBe(0);
    expect(
      currentBucketProgress(state, start + BUCKET_DURATION_MS / 2).percent,
    ).toBe(50);
    expect(
      currentBucketProgress(state, start + BUCKET_DURATION_MS + 1).percent,
    ).toBe(100);
  });
});
