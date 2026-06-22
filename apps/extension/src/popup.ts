import "./popup.css";

import { toCollectorStatusViewModel, type CollectorStatus } from "./status";
declare const browser: typeof chrome | undefined;

const extensionApi: typeof chrome =
  typeof browser === "undefined" ? chrome : browser;

interface PopupRequest {
  type:
    | "mindcanary.get_status"
    | "mindcanary.refresh_status"
    | "mindcanary.reset_queue"
    | "mindcanary.enable_scrolling";
}

const elements = {
  headline: requiredElement("headline"),
  detail: requiredElement("detail"),
  nextAction: requiredElement("next-action"),
  extensionId: requiredElement("extension-id"),
  nativeHost: requiredElement("native-host"),
  signals: requiredElement("signals"),
  queue: requiredElement("queue"),
  bucket: requiredElement("bucket"),
  bucketProgress: requiredElement("bucket-progress") as HTMLProgressElement,
  bucketProgressText: requiredElement("bucket-progress-text"),
  settings: requiredElement("settings"),
  delivery: requiredElement("delivery"),
  idlePermission: requiredElement("idle-permission"),
  scrollPermission: requiredElement("scroll-permission"),
  setup: requiredElement("setup"),
  setupCommand: requiredElement("setup-command"),
  requestIdle: requiredElement("request-idle") as HTMLButtonElement,
  requestScrolling: requiredElement("request-scrolling") as HTMLButtonElement,
  resetQueueCard: requiredElement("queue-reset-card"),
  resetQueue: requiredElement("reset-queue") as HTMLButtonElement,
  resetQueueCopy: requiredElement("reset-queue-copy"),
  refresh: requiredElement("refresh") as HTMLButtonElement,
};
const browserName =
  __MINDCANARY_BROWSER_TARGET__ === "firefox" ? "Firefox" : "Chrome";

elements.refresh.addEventListener("click", () => {
  void renderStatus("mindcanary.refresh_status");
});

elements.requestIdle.addEventListener("click", () => {
  void requestIdlePermission();
});

elements.requestScrolling.addEventListener("click", () => {
  void requestScrollingPermission();
});

elements.resetQueue.addEventListener("click", () => {
  void resetQueue();
});

void renderStatus("mindcanary.get_status");

async function renderStatus(type: PopupRequest["type"]): Promise<void> {
  elements.refresh.disabled = true;
  try {
    const status = await sendPopupRequest({ type });
    const model = toCollectorStatusViewModel(status);
    document.body.dataset.state = model.state;
    elements.headline.textContent = model.headline;
    elements.detail.textContent = model.detail;
    elements.nextAction.textContent = model.nextActionText;
    elements.extensionId.textContent = model.extensionIdText;
    elements.nativeHost.textContent = model.nativeHostText;
    elements.signals.textContent = model.enabledSignalText;
    elements.queue.textContent = model.pendingBatchText;
    elements.bucket.textContent = model.reducerText;
    elements.bucketProgressText.textContent = model.bucketProgressText;
    elements.bucketProgress.hidden = model.bucketProgressPercent === null;
    elements.bucketProgress.value = model.bucketProgressPercent ?? 0;
    elements.settings.textContent = model.settingsText;
    elements.delivery.textContent = model.deliveryText;
    elements.idlePermission.textContent = model.idlePermissionText;
    elements.scrollPermission.textContent = model.scrollPermissionText;
    elements.requestIdle.hidden = !model.showIdlePermissionRequest;
    elements.requestScrolling.hidden = !model.showScrollPermissionRequest;
    elements.resetQueueCard.hidden = !model.showQueueReset;
    elements.resetQueue.hidden = !model.showQueueReset;
    elements.resetQueueCopy.hidden = !model.showQueueReset;
    elements.resetQueueCopy.textContent = model.queueResetText;
    renderSetupCommand(model.setupCommand);
  } catch {
    document.body.dataset.state = "needs_setup";
    elements.headline.textContent = "Collector status unavailable";
    elements.detail.textContent = `${browserName} could not read the extension background status.`;
    elements.nextAction.textContent = `Reload the ${browserName} extension, then open this popup again.`;
    elements.extensionId.textContent = "-";
    elements.nativeHost.textContent = "-";
    elements.signals.textContent = "-";
    elements.queue.textContent = "-";
    elements.bucket.textContent = "-";
    elements.bucketProgressText.textContent = "-";
    elements.bucketProgress.hidden = true;
    elements.bucketProgress.value = 0;
    elements.settings.textContent = "-";
    elements.delivery.textContent = "-";
    elements.idlePermission.textContent = "-";
    elements.scrollPermission.textContent = "-";
    elements.requestIdle.hidden = true;
    elements.requestScrolling.hidden = true;
    elements.resetQueueCard.hidden = true;
    elements.resetQueue.hidden = true;
    elements.resetQueueCopy.hidden = true;
    elements.resetQueueCopy.textContent = "";
    renderSetupCommand(null);
  } finally {
    elements.refresh.disabled = false;
  }
}

async function requestScrollingPermission(): Promise<void> {
  elements.requestScrolling.disabled = true;
  try {
    const granted = await extensionApi.permissions.request({
      permissions: ["scripting"],
      origins: ["https://x.com/*", "https://twitter.com/*"],
    });
    await renderStatus(
      granted ? "mindcanary.enable_scrolling" : "mindcanary.refresh_status",
    );
  } finally {
    elements.requestScrolling.disabled = false;
  }
}

async function requestIdlePermission(): Promise<void> {
  elements.requestIdle.disabled = true;
  try {
    await extensionApi.permissions.request({ permissions: ["idle"] });
  } finally {
    await renderStatus("mindcanary.refresh_status");
    elements.requestIdle.disabled = false;
  }
}

async function resetQueue(): Promise<void> {
  elements.resetQueue.disabled = true;
  try {
    await renderStatus("mindcanary.reset_queue");
  } finally {
    elements.resetQueue.disabled = false;
  }
}

function sendPopupRequest(request: PopupRequest): Promise<CollectorStatus> {
  return extensionApi.runtime.sendMessage(request) as Promise<CollectorStatus>;
}

function requiredElement(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (element === null) {
    throw new Error(`Missing popup element: ${id}`);
  }
  return element;
}

function renderSetupCommand(command: string | null): void {
  if (command === null) {
    elements.setup.hidden = true;
    elements.setupCommand.textContent = "";
    return;
  }

  elements.setup.hidden = false;
  elements.setupCommand.textContent = command;
}
