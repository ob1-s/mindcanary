import {
  PROTOCOL_VERSION,
  type AggregateBatch,
  type ProtocolRequest,
  type ProtocolResponse,
  type SignalId,
} from "@mindcanary/protocol";

import { enabledSignalIds, filterEnabledMetrics } from "./collection-policy";
import {
  applyEvent,
  createReducerState,
  currentBucketProgress,
  type ActivityState,
  type CompletedBucket,
  type Reduction,
  type ReducerEvent,
  type ReducerState,
} from "./reducer";
import { trimPendingBatchQueue } from "./queue";
import {
  CONTINUOUS_SCROLLING_SIGNAL,
  advanceScrollContext,
  applyScrollContextEvent,
  createScrollContextState,
  type CompletedScrollBucket,
  type ScrollContextState,
} from "./scroll-context";
import type {
  ActiveBucketStatus,
  CollectorStatus,
  DeliveryStatus,
  SettingsStatus,
} from "./status";
import {
  observeTabRetention,
  TAB_RETENTION_SIGNAL,
  type TabRetentionState,
} from "./tab-retention";
import { uuidV7 } from "./uuid";
declare const browser: typeof chrome | undefined;

const extensionApi: typeof chrome =
  typeof browser === "undefined" ? chrome : browser;

const NATIVE_HOST = "app.mindcanary.collector";
const FLUSH_ALARM = "flush-aggregate-bucket";
const REDUCER_STATE_KEY = "reducerState";
const COLLECTOR_STATE_KEY = "collectorState";
const TAB_RETENTION_STATE_KEY = "tabRetentionState";
const ENABLED_SIGNALS_KEY = "enabledSignals";
const SETTINGS_STATUS_KEY = "settingsStatus";
const DELIVERY_STATUS_KEY = "deliveryStatus";
const SCROLL_CONTEXT_STATE_KEY = "scrollContextState";
const SCROLL_CONTENT_SCRIPT_ID = "mindcanary-continuous-scrolling";
const SCROLL_ORIGINS = ["https://x.com/*", "https://twitter.com/*"];

interface CollectorState {
  sourceInstanceId: string;
  nextSequence: number;
  pendingBatches: AggregateBatch[];
  droppedBatchCount: number;
}

interface PopupRequest {
  type:
    | "mindcanary.get_status"
    | "mindcanary.refresh_status"
    | "mindcanary.reset_queue"
    | "mindcanary.enable_scrolling";
}

interface ScrollActivityMessage {
  type: "mindcanary.scroll_activity";
  active: boolean;
  atMs: number;
}

let operation = Promise.resolve();

extensionApi.runtime.onInstalled.addListener(() => {
  enqueue(async () => {
    if (typeof extensionApi.storage.local.setAccessLevel === "function") {
      await extensionApi.storage.local.setAccessLevel({
        accessLevel: "TRUSTED_CONTEXTS",
      });
    }
    await extensionApi.alarms.create(FLUSH_ALARM, { periodInMinutes: 1 });
    const enabledSignals = await refreshCollectionSettings();
    if (enabledSignals.size > 0) {
      await reconcileSnapshot(enabledSignals);
    }
  });
});

extensionApi.runtime.onStartup.addListener(() => {
  enqueue(async () => {
    await extensionApi.alarms.create(FLUSH_ALARM, { periodInMinutes: 1 });
    const enabledSignals = await refreshCollectionSettings();
    if (enabledSignals.size > 0) {
      await reconcileSnapshot(enabledSignals);
    }
    await deliverPendingBatches();
  });
});

extensionApi.tabs.onCreated.addListener((tab) => {
  if (tab.id !== undefined) {
    enqueueEvent({ type: "tab_created", atMs: Date.now(), tabId: tab.id });
  }
});

extensionApi.tabs.onRemoved.addListener((tabId) => {
  enqueueEvent({ type: "tab_removed", atMs: Date.now(), tabId });
  enqueueScrollActivity(tabId, false, Date.now());
});

extensionApi.tabs.onActivated.addListener(({ tabId, windowId }) => {
  enqueueEvent({
    type: "tab_activated",
    atMs: Date.now(),
    tabId,
    windowId,
  });
});

extensionApi.windows.onCreated.addListener(() => {
  enqueueWindowCount();
});

extensionApi.windows.onRemoved.addListener(() => {
  enqueueWindowCount();
});

extensionApi.idle?.onStateChanged.addListener((state) => {
  const activity: ActivityState = state === "active" ? "active" : "idle";
  enqueueEvent({ type: "activity_changed", atMs: Date.now(), activity });
});

extensionApi.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name !== FLUSH_ALARM) {
    return;
  }

  enqueue(async () => {
    const enabledSignals = await refreshCollectionSettings();
    if (enabledSignals.size === 0) {
      await extensionApi.storage.session.remove(REDUCER_STATE_KEY);
      await extensionApi.storage.local.remove(TAB_RETENTION_STATE_KEY);
      await deliverPendingBatches();
      return;
    }

    const nowMs = Date.now();
    const reduction = await reconcileCurrentActivity(
      await loadOrCreateReducerState(),
      nowMs,
    );
    await persistReduction(
      reduction.state,
      reduction.completed,
      enabledSignals,
    );
    await persistTabRetentionObservation(
      reduction.state.openTabIds.length,
      nowMs,
      enabledSignals,
    );
    await flushScrollContext(enabledSignals, nowMs);
    await deliverPendingBatches();
  });
});

extensionApi.runtime.onMessage.addListener(
  (message: PopupRequest | ScrollActivityMessage, sender, sendResponse) => {
    if (message.type === "mindcanary.scroll_activity") {
      const tabId = sender.tab?.id;
      if (
        tabId !== undefined &&
        typeof message.active === "boolean" &&
        Number.isSafeInteger(message.atMs) &&
        message.atMs >= 0
      ) {
        enqueueScrollActivity(tabId, message.active, message.atMs);
      }
      return false;
    }
    if (
      message.type !== "mindcanary.get_status" &&
      message.type !== "mindcanary.refresh_status" &&
      message.type !== "mindcanary.reset_queue" &&
      message.type !== "mindcanary.enable_scrolling"
    ) {
      return false;
    }

    enqueue(async () => {
      if (message.type === "mindcanary.reset_queue") {
        await resetPendingBatches();
      } else if (message.type === "mindcanary.enable_scrolling") {
        await reconcileScrollAdapter(await loadEnabledSignals());
      } else if (message.type === "mindcanary.refresh_status") {
        const enabledSignals = await refreshCollectionSettings();
        if (enabledSignals.size === 0) {
          await extensionApi.storage.session.remove(REDUCER_STATE_KEY);
          await extensionApi.storage.local.remove(TAB_RETENTION_STATE_KEY);
        } else {
          const nowMs = Date.now();
          const reduction = await reconcileCurrentActivity(
            await loadOrCreateReducerState(),
            nowMs,
          );
          await persistReduction(
            reduction.state,
            reduction.completed,
            enabledSignals,
          );
          await persistTabRetentionObservation(
            reduction.state.openTabIds.length,
            nowMs,
            enabledSignals,
          );
          await flushScrollContext(enabledSignals, nowMs);
        }
        await deliverPendingBatches();
      }
      sendResponse(await collectorStatus());
    });
    return true;
  },
);

function enqueueEvent(event: ReducerEvent): void {
  enqueue(async () => {
    const enabledSignals = await loadEnabledSignals();
    if (enabledSignals.size === 0) {
      return;
    }
    const state = await loadOrCreateReducerState();
    const reduction = applyEvent(state, event);
    await persistReduction(
      reduction.state,
      reduction.completed,
      enabledSignals,
    );
    await persistTabRetentionObservation(
      reduction.state.openTabIds.length,
      event.atMs,
      enabledSignals,
    );
    await deliverPendingBatches();
  });
}

function enqueueScrollActivity(
  tabId: number,
  active: boolean,
  atMs: number,
): void {
  enqueue(async () => {
    const enabledSignals = await loadEnabledSignals();
    if (!enabledSignals.has(CONTINUOUS_SCROLLING_SIGNAL)) {
      return;
    }
    const state = await loadScrollContextState(atMs);
    const reduction = applyScrollContextEvent(state, {
      tabId,
      active,
      atMs: Math.max(atMs, state.lastEventMs),
    });
    await saveScrollContextState(reduction.state);
    await queueScrollBuckets(reduction.completed, enabledSignals);
    await deliverPendingBatches();
  });
}

function enqueueWindowCount(): void {
  enqueue(async () => {
    const enabledSignals = await loadEnabledSignals();
    if (enabledSignals.size === 0) {
      return;
    }
    const windows = await extensionApi.windows.getAll();
    const state = await loadOrCreateReducerState();
    const nowMs = Date.now();
    const reduction = applyEvent(state, {
      type: "window_count_changed",
      atMs: nowMs,
      windowCount: windows.length,
    });
    await persistReduction(
      reduction.state,
      reduction.completed,
      enabledSignals,
    );
    await persistTabRetentionObservation(
      reduction.state.openTabIds.length,
      nowMs,
      enabledSignals,
    );
  });
}

function enqueue(task: () => Promise<void>): void {
  operation = operation.then(task).catch(() => {
    // Payloads are intentionally excluded from production logging.
    console.error("collector_operation_failed");
  });
}

async function reconcileSnapshot(
  enabledSignals: ReadonlySet<SignalId>,
): Promise<void> {
  const [tabs, windows, activity] = await Promise.all([
    extensionApi.tabs.query({}),
    extensionApi.windows.getAll(),
    currentActivityState(),
  ]);
  const tabIds = tabs.flatMap((tab) => (tab.id === undefined ? [] : [tab.id]));
  const result = await extensionApi.storage.session.get(REDUCER_STATE_KEY);
  const existing = result[REDUCER_STATE_KEY] as ReducerState | undefined;
  const nowMs = Date.now();

  if (existing === undefined) {
    const state = createReducerState({
      nowMs,
      tabIds,
      windowCount: windows.length,
      activity,
      timeZone: resolvedTimeZone(),
    });
    await extensionApi.storage.session.set({ [REDUCER_STATE_KEY]: state });
    await persistTabRetentionObservation(
      state.openTabIds.length,
      nowMs,
      enabledSignals,
    );
    return;
  }

  const snapshotReduction = applyEvent(existing, {
    type: "snapshot",
    atMs: Math.max(nowMs, existing.lastEventMs),
    tabIds,
    windowCount: windows.length,
  });
  const activityReduction = applyEvent(snapshotReduction.state, {
    type: "activity_changed",
    atMs: snapshotReduction.state.lastEventMs,
    activity,
  });
  await persistReduction(
    activityReduction.state,
    [...snapshotReduction.completed, ...activityReduction.completed],
    enabledSignals,
  );
  await persistTabRetentionObservation(
    activityReduction.state.openTabIds.length,
    nowMs,
    enabledSignals,
  );
}

async function loadOrCreateReducerState(): Promise<ReducerState> {
  const result = await extensionApi.storage.session.get(REDUCER_STATE_KEY);
  const existing = result[REDUCER_STATE_KEY] as ReducerState | undefined;
  if (existing !== undefined) {
    return existing;
  }

  const [tabs, windows, activity] = await Promise.all([
    extensionApi.tabs.query({}),
    extensionApi.windows.getAll(),
    currentActivityState(),
  ]);
  return createReducerState({
    nowMs: Date.now(),
    tabIds: tabs.flatMap((tab) => (tab.id === undefined ? [] : [tab.id])),
    windowCount: windows.length,
    activity,
    timeZone: resolvedTimeZone(),
  });
}

async function reconcileCurrentActivity(
  state: ReducerState,
  nowMs: number,
): Promise<Reduction> {
  return applyEvent(state, {
    type: "activity_changed",
    atMs: Math.max(nowMs, state.lastEventMs),
    activity: await currentActivityState(),
  });
}

async function persistReduction(
  state: ReducerState,
  completed: CompletedBucket[],
  enabledSignals: ReadonlySet<SignalId>,
): Promise<void> {
  await extensionApi.storage.session.set({ [REDUCER_STATE_KEY]: state });
  if (completed.length === 0) {
    return;
  }

  const collector = await loadCollectorState();
  for (const bucket of completed) {
    const metrics = filterEnabledMetrics(bucket.metrics, enabledSignals);
    if (metrics.length === 0) {
      continue;
    }
    collector.pendingBatches.push({
      batch_id: uuidV7(bucket.endMs),
      source_instance_id: collector.sourceInstanceId,
      sequence: collector.nextSequence,
      period: {
        start: new Date(bucket.startMs).toISOString(),
        end: new Date(bucket.endMs).toISOString(),
        time_zone: resolvedTimeZone(),
      },
      metrics,
    });
    collector.nextSequence += 1;
  }

  trimPendingBatches(collector);
  await saveCollectorState(collector);
}

async function persistTabRetentionObservation(
  openTabCount: number,
  observedAtMs: number,
  enabledSignals: ReadonlySet<SignalId>,
): Promise<void> {
  if (!enabledSignals.has(TAB_RETENTION_SIGNAL)) {
    await extensionApi.storage.local.remove(TAB_RETENTION_STATE_KEY);
    return;
  }

  const result = await extensionApi.storage.local.get(TAB_RETENTION_STATE_KEY);
  const previous = result[TAB_RETENTION_STATE_KEY] as
    | TabRetentionState
    | undefined;
  const observation = observeTabRetention(
    previous,
    openTabCount,
    observedAtMs,
    resolvedTimeZone(),
  );
  await extensionApi.storage.local.set({
    [TAB_RETENTION_STATE_KEY]: observation.state,
  });

  if (observation.metric === undefined) {
    return;
  }

  await queueAggregateBatch({
    startMs: observedAtMs,
    endMs: observedAtMs + 1,
    metrics: [observation.metric],
    timeZone: resolvedTimeZone(),
  });
}

async function queueAggregateBatch({
  startMs,
  endMs,
  metrics,
  timeZone,
}: {
  startMs: number;
  endMs: number;
  metrics: AggregateBatch["metrics"];
  timeZone: string;
}): Promise<void> {
  const collector = await loadCollectorState();
  collector.pendingBatches.push({
    batch_id: uuidV7(endMs),
    source_instance_id: collector.sourceInstanceId,
    sequence: collector.nextSequence,
    period: {
      start: new Date(startMs).toISOString(),
      end: new Date(endMs).toISOString(),
      time_zone: timeZone,
    },
    metrics,
  });
  collector.nextSequence += 1;

  trimPendingBatches(collector);
  await saveCollectorState(collector);
}

function trimPendingBatches(collector: CollectorState): void {
  trimPendingBatchQueue(collector);
}

async function refreshCollectionSettings(): Promise<Set<SignalId>> {
  const request: ProtocolRequest = {
    type: "get_collection_settings",
    protocol_version: PROTOCOL_VERSION,
  };

  let response: ProtocolResponse;
  try {
    response = (await extensionApi.runtime.sendNativeMessage(
      NATIVE_HOST,
      request,
    )) as ProtocolResponse;
  } catch {
    await storeSettingsStatus("unavailable");
    return storeEnabledSignals([]);
  }

  if (response.type !== "collection_settings") {
    await storeSettingsStatus("unexpected_response");
    return storeEnabledSignals([]);
  }

  await storeSettingsStatus("ok");
  return storeEnabledSignals(enabledSignalIds(response.settings));
}

async function loadEnabledSignals(): Promise<Set<SignalId>> {
  const result = await extensionApi.storage.session.get(ENABLED_SIGNALS_KEY);
  const enabled = result[ENABLED_SIGNALS_KEY] as SignalId[] | undefined;
  return new Set(enabled ?? []);
}

async function storeEnabledSignals(
  enabledSignals: SignalId[],
): Promise<Set<SignalId>> {
  await extensionApi.storage.session.set({
    [ENABLED_SIGNALS_KEY]: enabledSignals,
  });
  const enabled = new Set(enabledSignals);
  await reconcileScrollAdapter(enabled);
  return enabled;
}

async function reconcileScrollAdapter(
  enabledSignals: Set<SignalId>,
): Promise<void> {
  const enabled = enabledSignals.has(CONTINUOUS_SCROLLING_SIGNAL);
  const permissionsGranted = await scrollingPermissionsGranted();
  if (!enabled || !permissionsGranted) {
    await extensionApi.storage.session.remove(SCROLL_CONTEXT_STATE_KEY);
    if (permissionsGranted) {
      const registered =
        await extensionApi.scripting.getRegisteredContentScripts({
          ids: [SCROLL_CONTENT_SCRIPT_ID],
        });
      if (registered.length > 0) {
        await extensionApi.scripting.unregisterContentScripts({
          ids: [SCROLL_CONTENT_SCRIPT_ID],
        });
      }
    }
    return;
  }

  const registered = await extensionApi.scripting.getRegisteredContentScripts({
    ids: [SCROLL_CONTENT_SCRIPT_ID],
  });
  if (registered.length === 0) {
    await extensionApi.scripting.registerContentScripts([
      {
        id: SCROLL_CONTENT_SCRIPT_ID,
        matches: SCROLL_ORIGINS,
        js: ["scroll-observer.js"],
        persistAcrossSessions: true,
        runAt: "document_start",
      },
    ]);
  }
}

async function scrollingPermissionsGranted(): Promise<boolean> {
  return extensionApi.permissions.contains({
    permissions: ["scripting"],
    origins: SCROLL_ORIGINS,
  });
}

async function loadScrollContextState(
  nowMs: number,
): Promise<ScrollContextState> {
  const result = await extensionApi.storage.session.get(
    SCROLL_CONTEXT_STATE_KEY,
  );
  return (
    (result[SCROLL_CONTEXT_STATE_KEY] as ScrollContextState | undefined) ??
    createScrollContextState(nowMs)
  );
}

async function saveScrollContextState(
  state: ScrollContextState,
): Promise<void> {
  await extensionApi.storage.session.set({
    [SCROLL_CONTEXT_STATE_KEY]: state,
  });
}

async function flushScrollContext(
  enabledSignals: Set<SignalId>,
  nowMs: number,
): Promise<void> {
  if (!enabledSignals.has(CONTINUOUS_SCROLLING_SIGNAL)) {
    await extensionApi.storage.session.remove(SCROLL_CONTEXT_STATE_KEY);
    return;
  }
  const state = await loadScrollContextState(nowMs);
  const reduction = advanceScrollContext(
    state,
    Math.max(nowMs, state.lastEventMs),
  );
  await saveScrollContextState(reduction.state);
  await queueScrollBuckets(reduction.completed, enabledSignals);
}

async function queueScrollBuckets(
  buckets: CompletedScrollBucket[],
  enabledSignals: Set<SignalId>,
): Promise<void> {
  if (
    buckets.length === 0 ||
    !enabledSignals.has(CONTINUOUS_SCROLLING_SIGNAL)
  ) {
    return;
  }
  const collector = await loadCollectorState();
  for (const bucket of buckets) {
    collector.pendingBatches.push({
      batch_id: uuidV7(bucket.endMs),
      source_instance_id: collector.sourceInstanceId,
      sequence: collector.nextSequence,
      period: {
        start: new Date(bucket.startMs).toISOString(),
        end: new Date(bucket.endMs).toISOString(),
        time_zone: resolvedTimeZone(),
      },
      metrics: [
        {
          signal: CONTINUOUS_SCROLLING_SIGNAL,
          value: bucket.seconds,
        },
      ],
    });
    collector.nextSequence += 1;
  }
  trimPendingBatches(collector);
  await saveCollectorState(collector);
}

async function deliverPendingBatches(): Promise<void> {
  const collector = await loadCollectorState();
  let delivered = 0;
  let deliveryStatus: DeliveryStatus["state"] = "ok";

  for (const batch of collector.pendingBatches) {
    const request: ProtocolRequest = {
      type: "ingest_aggregate",
      protocol_version: PROTOCOL_VERSION,
      batch,
    };

    let response: ProtocolResponse;
    try {
      response = (await extensionApi.runtime.sendNativeMessage(
        NATIVE_HOST,
        request,
      )) as ProtocolResponse;
    } catch {
      deliveryStatus = "unavailable";
      break;
    }

    if (
      response.type !== "ingest_acknowledged" ||
      response.batch_id !== batch.batch_id
    ) {
      deliveryStatus = "unexpected_response";
      break;
    }
    delivered += 1;
  }

  if (delivered > 0) {
    collector.pendingBatches.splice(0, delivered);
    await saveCollectorState(collector);
  }
  await storeDeliveryStatus(deliveryStatus, delivered);
}

async function loadCollectorState(): Promise<CollectorState> {
  const result = await extensionApi.storage.local.get(COLLECTOR_STATE_KEY);
  const existing = result[COLLECTOR_STATE_KEY] as CollectorState | undefined;
  if (existing !== undefined) {
    return {
      ...existing,
      droppedBatchCount: existing.droppedBatchCount ?? 0,
    };
  }

  const created: CollectorState = {
    sourceInstanceId: uuidV7(),
    nextSequence: 0,
    pendingBatches: [],
    droppedBatchCount: 0,
  };
  await saveCollectorState(created);
  return created;
}

async function saveCollectorState(state: CollectorState): Promise<void> {
  await extensionApi.storage.local.set({ [COLLECTOR_STATE_KEY]: state });
}

async function resetPendingBatches(): Promise<void> {
  const collector = await loadCollectorState();
  collector.pendingBatches = [];
  collector.droppedBatchCount = 0;
  await saveCollectorState(collector);
}

async function collectorStatus(): Promise<CollectorStatus> {
  const [
    collector,
    enabledSignals,
    session,
    idlePermissionGranted,
    scrollPermissionGranted,
  ] = await Promise.all([
    loadCollectorState(),
    loadEnabledSignals(),
    extensionApi.storage.session.get([
      REDUCER_STATE_KEY,
      SETTINGS_STATUS_KEY,
      DELIVERY_STATUS_KEY,
    ]),
    idlePermissionIsGranted(),
    scrollingPermissionsGranted(),
  ]);

  const reducerState = session[REDUCER_STATE_KEY] as ReducerState | undefined;

  return {
    browserTarget: __MINDCANARY_BROWSER_TARGET__,
    buildChannel: __MINDCANARY_EXTENSION_CHANNEL__,
    extensionId: extensionApi.runtime.id,
    expectedExtensionId: __MINDCANARY_EXPECTED_EXTENSION_ID__,
    identityMatches:
      extensionApi.runtime.id === __MINDCANARY_EXPECTED_EXTENSION_ID__,
    nativeHostName: NATIVE_HOST,
    enabledSignals: [...enabledSignals],
    idlePermissionGranted,
    scrollPermissionGranted,
    pendingBatchCount: collector.pendingBatches.length,
    droppedBatchCount: collector.droppedBatchCount,
    reducerActive: reducerState !== undefined,
    activeBucket: activeBucketStatus(reducerState, Date.now()),
    nextSequence: collector.nextSequence,
    settingsStatus: session[SETTINGS_STATUS_KEY] as SettingsStatus | undefined,
    deliveryStatus: session[DELIVERY_STATUS_KEY] as DeliveryStatus | undefined,
  };
}

function activeBucketStatus(
  state: ReducerState | undefined,
  nowMs: number,
): ActiveBucketStatus | undefined {
  if (state === undefined) {
    return undefined;
  }

  const progress = currentBucketProgress(state, nowMs);

  return {
    startedAt: new Date(progress.startMs).toISOString(),
    endsAt: new Date(progress.endMs).toISOString(),
    progressPercent: progress.percent,
  };
}

async function idlePermissionIsGranted(): Promise<boolean> {
  return extensionApi.permissions.contains({ permissions: ["idle"] });
}

async function currentActivityState(): Promise<ActivityState | null> {
  if (!(await idlePermissionIsGranted())) {
    return null;
  }

  try {
    const state = await extensionApi.idle.queryState(60);
    return state === "active" ? "active" : "idle";
  } catch {
    return null;
  }
}

async function storeSettingsStatus(
  state: SettingsStatus["state"],
): Promise<void> {
  await extensionApi.storage.session.set({
    [SETTINGS_STATUS_KEY]: {
      state,
      checkedAt: new Date().toISOString(),
    } satisfies SettingsStatus,
  });
}

async function storeDeliveryStatus(
  state: DeliveryStatus["state"],
  deliveredCount: number,
): Promise<void> {
  await extensionApi.storage.session.set({
    [DELIVERY_STATUS_KEY]: {
      state,
      checkedAt: new Date().toISOString(),
      deliveredCount,
    } satisfies DeliveryStatus,
  });
}

function resolvedTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
}
