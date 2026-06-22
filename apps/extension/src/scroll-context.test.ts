import { describe, expect, it } from "vitest";

import {
  advanceScrollContext,
  applyScrollContextEvent,
  createScrollContextState,
  type CompletedScrollBucket,
} from "./scroll-context";

const BUCKET = 15 * 60 * 1000;

describe("continuous scrolling reducer", () => {
  it("emits only aggregate duration for a complete bucket", () => {
    let state = createScrollContextState(0);
    state = applyScrollContextEvent(state, {
      atMs: 60_000,
      tabId: 4,
      active: true,
    }).state;
    state = advanceScrollContext(state, 120_000).state;
    state = applyScrollContextEvent(state, {
      atMs: 180_000,
      tabId: 4,
      active: false,
    }).state;

    let completed: CompletedScrollBucket[] = [];
    for (let now = 240_000; now <= BUCKET; now += 60_000) {
      const reduction = advanceScrollContext(state, now);
      state = reduction.state;
      completed = [...completed, ...reduction.completed];
    }
    expect(completed).toEqual([{ startMs: 0, endMs: BUCKET, seconds: 120 }]);
    expect(JSON.stringify({ state, completed })).not.toMatch(
      /url|title|route|content/i,
    );
  });

  it("counts overlapping tabs once and removes closed tabs independently", () => {
    let state = createScrollContextState(0);
    state = applyScrollContextEvent(state, {
      atMs: 0,
      tabId: 1,
      active: true,
    }).state;
    state = applyScrollContextEvent(state, {
      atMs: 10_000,
      tabId: 2,
      active: true,
    }).state;
    state = applyScrollContextEvent(state, {
      atMs: 20_000,
      tabId: 1,
      active: false,
    }).state;
    state = applyScrollContextEvent(state, {
      atMs: 30_000,
      tabId: 2,
      active: false,
    }).state;
    expect(advanceScrollContext(state, 40_000).state.activeMs).toBe(30_000);
  });

  it("drops partial starts and long interrupted intervals", () => {
    const partial = createScrollContextState(30_000);
    expect(advanceScrollContext(partial, BUCKET).completed).toEqual([]);

    let active = createScrollContextState(0);
    active = applyScrollContextEvent(active, {
      atMs: 0,
      tabId: 1,
      active: true,
    }).state;
    expect(advanceScrollContext(active, 120_000).completed).toEqual([]);
  });
});
