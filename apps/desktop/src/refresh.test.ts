import { describe, expect, it, vi } from "vitest";

import { createRefreshCoordinator, shouldAutoRefresh } from "./refresh";

describe("dashboard refresh coordination", () => {
  it("joins concurrent refresh requests instead of overlapping daemon reads", async () => {
    let finish: (() => void) | undefined;
    const refresh = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const coordinator = createRefreshCoordinator(refresh);

    const first = coordinator.run();
    const second = coordinator.run();
    await Promise.resolve();

    expect(first).toBe(second);
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(coordinator.isRunning()).toBe(true);

    finish?.();
    await first;
    expect(coordinator.isRunning()).toBe(false);
  });

  it("allows a later refresh after failure", async () => {
    const refresh = vi
      .fn<() => Promise<void>>()
      .mockRejectedValueOnce(new Error("daemon unavailable"))
      .mockResolvedValueOnce();
    const coordinator = createRefreshCoordinator(refresh);

    await expect(coordinator.run()).rejects.toThrow("daemon unavailable");
    await expect(coordinator.run()).resolves.toBeUndefined();
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("polls only while the dashboard is visible", () => {
    expect(shouldAutoRefresh("visible")).toBe(true);
    expect(shouldAutoRefresh("hidden")).toBe(false);
    expect(shouldAutoRefresh("prerender")).toBe(false);
  });
});
