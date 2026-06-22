interface ScrollActivityMessage {
  type: "mindcanary.scroll_activity";
  active: boolean;
  atMs: number;
}

declare const browser: typeof chrome | undefined;

const extensionApi: typeof chrome =
  typeof browser === "undefined" ? chrome : browser;
const SCROLLING_SETTLE_MS = 3_000;
let active = false;
let settleTimer: number | undefined;

function markScrolling(): void {
  if (document.visibilityState !== "visible") {
    return;
  }
  if (!active) {
    active = true;
    sendActivity(true);
  }
  if (settleTimer !== undefined) {
    window.clearTimeout(settleTimer);
  }
  settleTimer = window.setTimeout(stopScrolling, SCROLLING_SETTLE_MS);
}

function stopScrolling(): void {
  if (settleTimer !== undefined) {
    window.clearTimeout(settleTimer);
    settleTimer = undefined;
  }
  if (active) {
    active = false;
    sendActivity(false);
  }
}

function sendActivity(isActive: boolean): void {
  const message: ScrollActivityMessage = {
    type: "mindcanary.scroll_activity",
    active: isActive,
    atMs: Date.now(),
  };
  void extensionApi.runtime.sendMessage(message).catch(() => undefined);
}

function scrollingKey(event: KeyboardEvent): boolean {
  return [
    "ArrowDown",
    "ArrowUp",
    "End",
    "Home",
    "PageDown",
    "PageUp",
    " ",
  ].includes(event.key);
}

document.addEventListener("wheel", markScrolling, { passive: true });
document.addEventListener("touchmove", markScrolling, { passive: true });
document.addEventListener("keydown", (event) => {
  if (scrollingKey(event)) {
    markScrolling();
  }
});
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState !== "visible") {
    stopScrolling();
  }
});
window.addEventListener("pagehide", stopScrolling);
