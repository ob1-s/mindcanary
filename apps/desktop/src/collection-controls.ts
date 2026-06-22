import {
  BROWSER_STARTER_SIGNAL_IDS,
  PROTOCOL_VERSION,
  SIGNAL_IDS,
  type ProtocolRequest,
  type ProtocolResponse,
  type SignalCollectionSetting,
  type SignalId,
} from "@mindcanary/protocol";

export interface SignalCollectionControlModel {
  signal: SignalId;
  label: string;
  description: string;
  enabled: boolean;
  statusText: string;
}

export interface SignalDeletionConfirmationModel {
  signal: SignalId;
  label: string;
  metricRecordCount: number;
  affectedBatchCount: number;
  summaryText: string;
  confirmationText: string;
  confirmationToken: string;
  expiresAt: string;
  isEmpty: boolean;
}

export interface SignalDeletionResultModel {
  signal: SignalId;
  label: string;
  summaryText: string;
}

export interface BrowserStarterSetModel {
  signals: SignalId[];
  labels: string[];
  enabledCount: number;
  totalCount: number;
  fullyEnabled: boolean;
  statusText: string;
}

export interface SignalGroupUpdateResult {
  attemptedCount: number;
  enabledCount: number;
  failedSignals: SignalId[];
}

export const BROWSER_STARTER_SIGNALS: SignalId[] = [
  ...BROWSER_STARTER_SIGNAL_IDS,
];

export const OS_ACTIVITY_SIGNALS: SignalId[] = [
  "os.active_seconds",
  "os.idle_seconds",
];

export const OS_LIFECYCLE_SIGNALS: SignalId[] = [
  "os.lock_count",
  "os.unlock_count",
  "os.suspend_count",
  "os.resume_count",
];

const SIGNAL_COPY: Record<SignalId, { label: string; description: string }> = {
  "browser.tab_switch_count": {
    label: "Tab switching",
    description: "Counts changes between active tabs.",
  },
  "browser.open_tab_count_min": {
    label: "Minimum open tabs",
    description: "Records the lowest open-tab count in each 15-minute period.",
  },
  "browser.open_tab_count_max": {
    label: "Maximum open tabs",
    description: "Records the highest open-tab count in each 15-minute period.",
  },
  "browser.open_tab_count_mean": {
    label: "Average open tabs",
    description: "Records the time-weighted average open-tab count.",
  },
  "browser.tab_open_count": {
    label: "Tabs opened",
    description: "Counts tabs opened in each 15-minute period.",
  },
  "browser.tab_close_count": {
    label: "Tabs closed",
    description: "Counts tabs closed in each 15-minute period.",
  },
  "browser.window_count_max": {
    label: "Browser windows",
    description: "Records the highest browser-window count in each period.",
  },
  "browser.active_seconds": {
    label: "Active browser time",
    description: "Measures browser time while the browser reports active use.",
  },
  "browser.idle_seconds": {
    label: "Idle browser time",
    description: "Measures browser time while the browser reports idle use.",
  },
  "browser.retained_across_day_count": {
    label: "Tabs retained across days",
    description: "Counts tabs that remain open across a local day boundary.",
  },
  "browser.continuous_scrolling_seconds": {
    label: "Continuous scrolling time",
    description:
      "Measures scrolling duration only on feed sites you explicitly allow; no route or content is stored.",
  },
  "os.active_seconds": {
    label: "Computer active time",
    description:
      "Measures device active time without app-specific content details.",
  },
  "os.idle_seconds": {
    label: "Computer idle time",
    description: "Measures device idle time without app or content details.",
  },
  "os.lock_count": {
    label: "Screen locks",
    description: "Counts lock events reported by the local OS adapter.",
  },
  "os.unlock_count": {
    label: "Screen unlocks",
    description: "Counts unlock events reported by the local OS adapter.",
  },
  "os.suspend_count": {
    label: "Suspends",
    description: "Counts suspend events reported by the local OS adapter.",
  },
  "os.resume_count": {
    label: "Resumes",
    description: "Counts resume events reported by the local OS adapter.",
  },
};

export function createCollectionSettingsRequest(): ProtocolRequest {
  return {
    type: "get_collection_settings",
    protocol_version: PROTOCOL_VERSION,
  };
}

export function createSetSignalCollectionRequest(
  signal: SignalId,
  enabled: boolean,
): ProtocolRequest {
  return {
    type: "set_signal_collection",
    protocol_version: PROTOCOL_VERSION,
    signal,
    enabled,
  };
}

export function createPrepareDeleteSignalRecordsRequest(
  signal: SignalId,
): ProtocolRequest {
  return {
    type: "prepare_delete_signal_records",
    protocol_version: PROTOCOL_VERSION,
    signal,
  };
}

export function createDeleteSignalRecordsRequest(
  signal: SignalId,
  confirmationToken: string,
): ProtocolRequest {
  if (confirmationToken.length === 0) {
    throw new TypeError("A confirmation token is required.");
  }

  return {
    type: "delete_signal_records",
    protocol_version: PROTOCOL_VERSION,
    signal,
    confirmation_token: confirmationToken,
  };
}

export function toSignalCollectionControls(
  response: ProtocolResponse,
): SignalCollectionControlModel[] {
  if (response.type !== "collection_settings") {
    throw new TypeError("Unexpected response for collection controls.");
  }

  const bySignal = new Map(
    response.settings.map((setting) => [setting.signal, setting]),
  );
  return SIGNAL_IDS.map((signal) =>
    toControl(
      bySignal.get(signal) ?? {
        signal,
        enabled: false,
        changed_at: null,
      },
    ),
  );
}

export function toSignalDeletionConfirmation(
  response: ProtocolResponse,
): SignalDeletionConfirmationModel {
  if (response.type !== "delete_signal_records_confirmation") {
    throw new TypeError("Unexpected signal deletion confirmation response.");
  }
  const label = SIGNAL_COPY[response.signal].label;
  const isEmpty = response.summary.metric_record_count === 0;
  return {
    signal: response.signal,
    label,
    metricRecordCount: response.summary.metric_record_count,
    affectedBatchCount: response.summary.affected_batch_count,
    summaryText: formatSignalRecordSummary(
      response.summary.metric_record_count,
      response.summary.affected_batch_count,
    ),
    confirmationText: isEmpty
      ? `There are no stored ${label.toLowerCase()} values to delete.`
      : `This removes only stored ${label.toLowerCase()} values. It does not delete check-ins or other signals, and it does not change future collection.`,
    confirmationToken: response.confirmation_token,
    expiresAt: response.expires_at,
    isEmpty,
  };
}

export function toSignalDeletionResult(
  response: ProtocolResponse,
): SignalDeletionResultModel {
  if (response.type !== "signal_records_deleted") {
    throw new TypeError("Unexpected signal deletion response.");
  }
  return {
    signal: response.signal,
    label: SIGNAL_COPY[response.signal].label,
    summaryText: formatSignalRecordSummary(
      response.deleted.metric_record_count,
      response.deleted.affected_batch_count,
    ),
  };
}

export function browserCollectionControls(
  controls: SignalCollectionControlModel[],
): SignalCollectionControlModel[] {
  return controls.filter((control) => control.signal.startsWith("browser."));
}

export function osActivityCollectionControls(
  controls: SignalCollectionControlModel[],
): SignalCollectionControlModel[] {
  const osActivitySignals = new Set(OS_ACTIVITY_SIGNALS);
  return controls.filter((control) => osActivitySignals.has(control.signal));
}

export function osCollectionControls(
  controls: SignalCollectionControlModel[],
): SignalCollectionControlModel[] {
  return controls.filter((control) => control.signal.startsWith("os."));
}

export function toBrowserStarterSetModel(
  controls: SignalCollectionControlModel[],
): BrowserStarterSetModel {
  return toStarterSetModel(controls, BROWSER_STARTER_SIGNALS);
}

export function toOsActivityStarterSetModel(
  controls: SignalCollectionControlModel[],
): BrowserStarterSetModel {
  return toStarterSetModel(controls, OS_ACTIVITY_SIGNALS);
}

function toStarterSetModel(
  controls: SignalCollectionControlModel[],
  signals: SignalId[],
): BrowserStarterSetModel {
  const enabled = new Set(
    controls
      .filter((control) => control.enabled)
      .map((control) => control.signal),
  );
  const enabledCount = signals.filter((signal) => enabled.has(signal)).length;
  const totalCount = signals.length;

  return {
    signals: [...signals],
    labels: signals.map((signal) => SIGNAL_COPY[signal].label),
    enabledCount,
    totalCount,
    fullyEnabled: enabledCount === totalCount,
    statusText:
      enabledCount === totalCount
        ? "Starter set enabled"
        : `${enabledCount} of ${totalCount} starter signals enabled`,
  };
}

export async function enableBrowserStarterSet(
  controls: SignalCollectionControlModel[],
  enableSignal: (signal: SignalId) => Promise<void>,
): Promise<SignalGroupUpdateResult> {
  return enableSignalGroup(controls, BROWSER_STARTER_SIGNALS, enableSignal);
}

export async function enableOsActivityStarterSet(
  controls: SignalCollectionControlModel[],
  enableSignal: (signal: SignalId) => Promise<void>,
): Promise<SignalGroupUpdateResult> {
  return enableSignalGroup(controls, OS_ACTIVITY_SIGNALS, enableSignal);
}

async function enableSignalGroup(
  controls: SignalCollectionControlModel[],
  signals: SignalId[],
  enableSignal: (signal: SignalId) => Promise<void>,
): Promise<SignalGroupUpdateResult> {
  const enabled = new Set(
    controls
      .filter((control) => control.enabled)
      .map((control) => control.signal),
  );
  const missing = signals.filter((signal) => !enabled.has(signal));
  const failedSignals: SignalId[] = [];
  let enabledCount = 0;

  for (const signal of missing) {
    try {
      await enableSignal(signal);
      enabledCount += 1;
    } catch {
      failedSignals.push(signal);
    }
  }

  return {
    attemptedCount: missing.length,
    enabledCount,
    failedSignals,
  };
}

function toControl(
  setting: SignalCollectionSetting,
): SignalCollectionControlModel {
  const copy = SIGNAL_COPY[setting.signal];
  return {
    signal: setting.signal,
    label: copy.label,
    description: copy.description,
    enabled: setting.enabled,
    statusText:
      setting.changed_at == null
        ? "Never enabled on this device"
        : `${setting.enabled ? "Enabled" : "Paused"} locally at ${setting.changed_at}`,
  };
}

function formatSignalRecordSummary(
  metricRecordCount: number,
  affectedBatchCount: number,
): string {
  return [
    pluralize(metricRecordCount, "stored value", "stored values"),
    pluralize(affectedBatchCount, "aggregate period", "aggregate periods"),
  ].join(" across ");
}

function pluralize(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}
