export const DASHBOARD_AUTO_REFRESH_INTERVAL_MS = 60_000;

export interface RefreshCoordinator {
  run: () => Promise<void>;
  isRunning: () => boolean;
}

export function createRefreshCoordinator(
  refresh: () => Promise<void>,
): RefreshCoordinator {
  let inFlight: Promise<void> | undefined;

  return {
    run() {
      if (inFlight !== undefined) {
        return inFlight;
      }

      const operation = Promise.resolve()
        .then(refresh)
        .finally(() => {
          if (inFlight === operation) {
            inFlight = undefined;
          }
        });
      inFlight = operation;
      return operation;
    },
    isRunning() {
      return inFlight !== undefined;
    },
  };
}

export function shouldAutoRefresh(
  visibilityState: DocumentVisibilityState,
): boolean {
  return visibilityState === "visible";
}
