import type { SignalId } from "@mindcanary/protocol";

export type SettingsStatusState = "ok" | "unavailable" | "unexpected_response";
export type DeliveryStatusState = "ok" | "unavailable" | "unexpected_response";

export interface SettingsStatus {
  state: SettingsStatusState;
  checkedAt: string;
}

export interface DeliveryStatus {
  state: DeliveryStatusState;
  checkedAt: string;
  deliveredCount: number;
}

export interface ActiveBucketStatus {
  startedAt: string;
  endsAt: string;
  progressPercent: number;
}

export interface CollectorStatus {
  browserTarget: "chrome" | "firefox";
  buildChannel: "development" | "release";
  extensionId: string;
  expectedExtensionId: string;
  identityMatches: boolean;
  nativeHostName: string;
  enabledSignals: SignalId[];
  idlePermissionGranted: boolean;
  scrollPermissionGranted: boolean;
  pendingBatchCount: number;
  droppedBatchCount: number;
  reducerActive: boolean;
  activeBucket?: ActiveBucketStatus;
  nextSequence: number;
  settingsStatus?: SettingsStatus;
  deliveryStatus?: DeliveryStatus;
}

export interface CollectorStatusViewModel {
  state: "collecting" | "disabled" | "needs_setup" | "queued" | "idle";
  headline: string;
  detail: string;
  nextActionText: string;
  enabledSignalText: string;
  pendingBatchText: string;
  reducerText: string;
  bucketProgressPercent: number | null;
  bucketProgressText: string;
  deliveryText: string;
  settingsText: string;
  idlePermissionText: string;
  showIdlePermissionRequest: boolean;
  scrollPermissionText: string;
  showScrollPermissionRequest: boolean;
  showQueueReset: boolean;
  queueResetText: string;
  extensionIdText: string;
  nativeHostText: string;
  setupCommand: string | null;
}

export function toCollectorStatusViewModel(
  status: CollectorStatus,
): CollectorStatusViewModel {
  const enabledSignalText = pluralize(
    status.enabledSignals.length,
    "enabled signal",
    "enabled signals",
  );
  const browserName = status.browserTarget === "firefox" ? "Firefox" : "Chrome";
  const pendingBatchText = queueStatusText(
    status.pendingBatchCount,
    status.droppedBatchCount,
  );
  const reducerText = status.reducerActive
    ? "Current 15-minute bucket is active"
    : "No active bucket in this browser session";
  const bucketProgressPercent = status.activeBucket?.progressPercent ?? null;
  const bucketProgressText = activeBucketStatusText(status.activeBucket);
  const settingsText = settingsStatusText(status.settingsStatus);
  const deliveryText = deliveryStatusText(status.deliveryStatus);
  const needsIdlePermission = requiresIdlePermission(status.enabledSignals);
  const idlePermissionText = idlePermissionStatusText(
    needsIdlePermission,
    status.idlePermissionGranted,
  );
  const showIdlePermissionRequest =
    needsIdlePermission && !status.idlePermissionGranted;
  const needsScrollPermission = status.enabledSignals.includes(
    "browser.continuous_scrolling_seconds",
  );
  const showScrollPermissionRequest =
    needsScrollPermission && !status.scrollPermissionGranted;
  const scrollPermissionText = !needsScrollPermission
    ? "Feed scrolling adapter is not enabled"
    : status.scrollPermissionGranted
      ? "Selected feed-site permission granted"
      : "Optional x.com and twitter.com access is needed";
  const showQueueReset =
    status.pendingBatchCount > 0 || status.droppedBatchCount > 0;
  const queueResetText = `Clear only the extension's unsent aggregate queue. Delivered local records and ${browserName} extension installation are unchanged.`;
  const nextActionText = nextActionStatusText(
    status,
    showIdlePermissionRequest,
    showScrollPermissionRequest,
  );
  const extensionIdText = status.extensionId;
  const nativeHostText = status.nativeHostName;
  const setupCommand = nativeHostInstallCommand(status);

  if (!status.identityMatches) {
    return {
      state: "needs_setup",
      headline: "Extension identity mismatch",
      detail: `This ${browserName} installation does not match the identity selected when MindCanary was built.`,
      nextActionText:
        "Remove this unpacked extension and load the current MindCanary build again.",
      enabledSignalText,
      pendingBatchText,
      reducerText,
      bucketProgressPercent,
      bucketProgressText,
      deliveryText,
      settingsText,
      idlePermissionText,
      showIdlePermissionRequest,
      scrollPermissionText,
      showScrollPermissionRequest,
      showQueueReset,
      queueResetText,
      extensionIdText,
      nativeHostText,
      setupCommand: null,
    };
  }

  if (status.settingsStatus?.state === "unavailable") {
    return {
      state: "needs_setup",
      headline: "Native host not connected",
      detail: `Start the local daemon and install the native-host manifest for this ${browserName} extension ID.`,
      nextActionText,
      enabledSignalText,
      pendingBatchText,
      reducerText,
      bucketProgressPercent,
      bucketProgressText,
      deliveryText,
      settingsText,
      idlePermissionText,
      showIdlePermissionRequest,
      scrollPermissionText,
      showScrollPermissionRequest,
      showQueueReset,
      queueResetText,
      extensionIdText,
      nativeHostText,
      setupCommand,
    };
  }

  if (status.enabledSignals.length === 0) {
    return {
      state: "disabled",
      headline: "Collection is paused",
      detail: "Enable one or more aggregate signals in the desktop app.",
      nextActionText,
      enabledSignalText,
      pendingBatchText,
      reducerText,
      bucketProgressPercent,
      bucketProgressText,
      deliveryText,
      settingsText,
      idlePermissionText,
      showIdlePermissionRequest,
      scrollPermissionText,
      showScrollPermissionRequest,
      showQueueReset,
      queueResetText,
      extensionIdText,
      nativeHostText,
      setupCommand: null,
    };
  }

  if (status.pendingBatchCount > 0) {
    return {
      state: "queued",
      headline: "Aggregates are queued",
      detail:
        "MindCanary has local aggregate batches waiting for the daemon to acknowledge them.",
      nextActionText,
      enabledSignalText,
      pendingBatchText,
      reducerText,
      bucketProgressPercent,
      bucketProgressText,
      deliveryText,
      settingsText,
      idlePermissionText,
      showIdlePermissionRequest,
      scrollPermissionText,
      showScrollPermissionRequest,
      showQueueReset,
      queueResetText,
      extensionIdText,
      nativeHostText,
      setupCommand: null,
    };
  }

  if (status.reducerActive) {
    return {
      state: "collecting",
      headline: "Collecting local aggregates",
      detail:
        "The extension is building aggregate buckets. No URLs, titles, page text, or history are collected.",
      nextActionText,
      enabledSignalText,
      pendingBatchText,
      reducerText,
      bucketProgressPercent,
      bucketProgressText,
      deliveryText,
      settingsText,
      idlePermissionText,
      showIdlePermissionRequest,
      scrollPermissionText,
      showScrollPermissionRequest,
      showQueueReset,
      queueResetText,
      extensionIdText,
      nativeHostText,
      setupCommand: null,
    };
  }

  return {
    state: "idle",
    headline: "Ready, waiting for activity",
    detail:
      "Enabled aggregate signals are loaded. Browser events or the next alarm will start a bucket.",
    nextActionText,
    enabledSignalText,
    pendingBatchText,
    reducerText,
    bucketProgressPercent,
    bucketProgressText,
    deliveryText,
    settingsText,
    idlePermissionText,
    showIdlePermissionRequest,
    scrollPermissionText,
    showScrollPermissionRequest,
    showQueueReset,
    queueResetText,
    extensionIdText,
    nativeHostText,
    setupCommand: null,
  };
}

function activeBucketStatusText(
  bucket: ActiveBucketStatus | undefined,
): string {
  if (bucket === undefined) {
    return "No bucket is being built yet";
  }

  const endsAt = formatTimestamp(bucket.endsAt);
  return `${bucket.progressPercent}% complete; closes at ${endsAt}`;
}

function queueStatusText(queued: number, dropped: number): string {
  const queuedText = pluralize(queued, "queued batch", "queued batches");
  if (dropped === 0) {
    return queuedText;
  }

  return `${queuedText}; ${pluralize(
    dropped,
    "dropped after queue limit",
    "dropped after queue limit",
  )}`;
}

function nextActionStatusText(
  status: CollectorStatus,
  showIdlePermissionRequest: boolean,
  showScrollPermissionRequest: boolean,
): string {
  if (status.settingsStatus?.state === "unavailable") {
    return "Start mindcanaryd, run the native-host setup command below, reload the extension, then refresh.";
  }
  if (status.enabledSignals.length === 0) {
    return "Enable browser aggregates in the desktop app or run mindcanaryctl enable-browser-defaults.";
  }
  if (showIdlePermissionRequest) {
    return "Click Allow idle permission so active and idle browser time can be counted.";
  }
  if (showScrollPermissionRequest) {
    return "Allow the optional selected feed-site permission to measure continuous scrolling duration.";
  }
  if (status.pendingBatchCount > 0) {
    return "Keep the local service running; queued aggregates will deliver on refresh or the next retry.";
  }
  if (status.reducerActive) {
    return `Leave ${status.browserTarget === "firefox" ? "Firefox" : "Chrome"} open; the extension will flush local aggregates into 15-minute buckets.`;
  }
  return `Use ${status.browserTarget === "firefox" ? "Firefox" : "Chrome"} normally or press Refresh status after opening, closing, or switching tabs.`;
}

function nativeHostInstallCommand(status: CollectorStatus): string {
  return [
    "cargo build -p mindcanary-native-host",
    "cargo run -p mindcanary-native-host -- \\",
    "  --install-manifest \\",
    `  --browser ${status.browserTarget} \\`,
    `  --channel ${status.buildChannel} \\`,
    '  --host-path "$PWD/target/debug/mindcanary-native-host"',
  ].join("\n");
}

function settingsStatusText(status: SettingsStatus | undefined): string {
  if (status === undefined) {
    return "Settings have not been checked yet";
  }
  const checkedAt = formatTimestamp(status.checkedAt);
  switch (status.state) {
    case "ok":
      return `Settings loaded at ${checkedAt}`;
    case "unavailable":
      return `Settings unavailable at ${checkedAt}`;
    case "unexpected_response":
      return `Unexpected settings response at ${checkedAt}`;
  }
}

function deliveryStatusText(status: DeliveryStatus | undefined): string {
  if (status === undefined) {
    return "No delivery attempt yet";
  }
  const checkedAt = formatTimestamp(status.checkedAt);
  switch (status.state) {
    case "ok":
      return `${pluralize(status.deliveredCount, "batch", "batches")} delivered at ${checkedAt}`;
    case "unavailable":
      return `Daemon delivery unavailable at ${checkedAt}`;
    case "unexpected_response":
      return `Unexpected delivery response at ${checkedAt}`;
  }
}

function requiresIdlePermission(signals: SignalId[]): boolean {
  return (
    signals.includes("browser.active_seconds") ||
    signals.includes("browser.idle_seconds")
  );
}

function idlePermissionStatusText(required: boolean, granted: boolean): string {
  if (granted) {
    return "Idle permission granted";
  }
  if (required) {
    return "Idle permission needed for browser active/idle aggregates";
  }
  return "Idle permission not needed for enabled signals";
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function pluralize(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}
