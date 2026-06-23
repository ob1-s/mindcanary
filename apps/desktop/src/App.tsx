import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type FormEvent,
  type ReactNode,
} from "react";

import {
  PROTOCOL_VERSION,
  type AnnotationRecord,
  type CheckInRecord,
  type ContextTag,
  type SignalId,
} from "@mindcanary/protocol";

import { AnnotationDialog } from "./annotation-dialog";
import { Dialog } from "./dialog";
import {
  annotationDraft,
  emptyAnnotationDraft,
  toAnnotationDeletionConfirmation,
  type AnnotationDraft,
  type AnnotationDeletionConfirmationModel,
} from "./annotation";
import {
  toBackupConfirmationModel,
  toCreatedBackupModel,
  toRestoredBackupModel,
  toVerifiedBackupModel,
  type BackupConfirmationModel,
  type CreatedBackupModel,
  type RestoredBackupModel,
  type VerifiedBackupModel,
} from "./backup";

import {
  CONTEXT_TAG_LABELS,
  EMPTY_CHECK_IN_DRAFT,
  contextTagOptions,
  createSubmitCheckInRequest,
  hasCheckInAnswers,
  toggleContextTag,
  type CheckInDraft,
} from "./check-in";
import {
  browserCollectionControls,
  enableBrowserStarterSet,
  enableOsActivityStarterSet,
  osActivityCollectionControls,
  osCollectionControls,
  toBrowserStarterSetModel,
  toOsActivityStarterSetModel,
  toSignalDeletionConfirmation,
  toSignalDeletionResult,
  toSignalCollectionControls,
  type SignalDeletionConfirmationModel,
  type SignalCollectionControlModel,
} from "./collection-controls";
import {
  daemonApi,
  type LocalServiceAutostartStatus,
} from "./daemon-api";
import {
  createSupportDiagnostics,
  toLocalDataControlModel,
  type ClearLocalRecordsConfirmationModel,
  type ExportLocalRecordsConfirmationModel,
  type LocalDataControlModel,
  type SupportDiagnosticsModel,
} from "./data-controls";
import {
  toDailyRhythmDashboardModel,
  type DailyRhythmDashboardModel,
  type ReadinessItemModel,
} from "./insights";
import {
  localRemovalModel,
  localRemovalResultText,
  type LocalRemovalModel,
} from "./local-removal";
import {
  ONBOARDING_STORAGE_KEY,
  toOnboardingModel,
  type OnboardingStep,
} from "./onboarding";
import {
  toPlatformCapabilityModel,
  type PlatformCapabilityCardModel,
  type PlatformCapabilityModel,
} from "./platform";
import {
  DASHBOARD_AUTO_REFRESH_INTERVAL_MS,
  createRefreshCoordinator,
  shouldAutoRefresh,
  type RefreshCoordinator,
} from "./refresh";
import {
  toSetupChecklistModel,
  type LocalServiceState,
  type SetupChecklistModel,
} from "./setup";
import {
  daemonConnectionItem,
  toConnectionStatusModel,
  type ConnectionStatusItemModel,
  type ConnectionStatusModel,
} from "./source-status";
import {
  BROWSER_TIMELINE_SOURCE,
  CHECK_IN_TIMELINE_SOURCE,
  OS_TIMELINE_SOURCE,
  toDailyTimelineDashboardModel,
  toLatestLocalRecordModel,
  toPriorCheckInReferences,
  type DailyTimelineDashboardModel,
  type PriorCheckInReference,
  type TimelineDayModel,
} from "./timeline";

type AppSection = "today" | "history" | "sources" | "data";

export function App() {
  const [serviceState, setServiceState] =
    useState<LocalServiceState>("checking");
  const [activeSection, setActiveSection] = useState<AppSection>("today");
  const [onboardingCompleted, setOnboardingCompleted] = useState(
    readOnboardingCompleted,
  );
  const [insights, setInsights] = useState<DailyRhythmDashboardModel>();
  const [timeline, setTimeline] = useState<DailyTimelineDashboardModel>();
  const [connections, setConnections] = useState<ConnectionStatusModel>();
  const [collection, setCollection] = useState<SignalCollectionControlModel[]>(
    [],
  );
  const [platform, setPlatform] = useState<PlatformCapabilityModel>();
  const [collectionUnavailable, setCollectionUnavailable] = useState(false);
  const [localServiceAutostart, setLocalServiceAutostart] =
    useState<LocalServiceAutostartStatus>();
  const [autostartUpdating, setAutostartUpdating] = useState(false);
  const [dataControl, setDataControl] = useState<LocalDataControlModel>();
  const [dataUnavailable, setDataUnavailable] = useState(false);
  const [clearConfirmation, setClearConfirmation] =
    useState<ClearLocalRecordsConfirmationModel>();
  const [exportConfirmation, setExportConfirmation] =
    useState<ExportLocalRecordsConfirmationModel>();
  const [backupConfirmation, setBackupConfirmation] =
    useState<BackupConfirmationModel>();
  const [createdBackup, setCreatedBackup] = useState<CreatedBackupModel>();
  const [restoreBackupOpen, setRestoreBackupOpen] = useState(false);
  const [localRemoval, setLocalRemoval] = useState<LocalRemovalModel>();
  const [signalDeletion, setSignalDeletion] =
    useState<SignalDeletionConfirmationModel>();
  const [annotationDeletion, setAnnotationDeletion] =
    useState<AnnotationDeletionConfirmationModel>();
  const [notice, setNotice] = useState<string>();
  const [refreshing, setRefreshing] = useState(false);
  const [connectorConnecting, setConnectorConnecting] = useState(false);
  const refreshCoordinator = useRef<RefreshCoordinator | null>(null);
  const localServiceSetupAttempted = useRef(false);
  const [annotationDraftState, setAnnotationDraftState] =
    useState<AnnotationDraft>();
  const [appVersion, setAppVersion] = useState<string>();
  const [supportDiagnostics, setSupportDiagnostics] =
    useState<SupportDiagnosticsModel>();

  async function submitAnnotation(annotation: AnnotationRecord) {
    try {
      await daemonApi.saveAnnotation(annotation);
      setNotice(
        annotationDraftState?.annotationId === undefined
          ? "Note saved on this device."
          : "Note updated.",
      );
      setAnnotationDraftState(undefined);
      await refreshDashboard();
    } catch (caught) {
      throw new Error("The note could not be saved.", { cause: caught });
    }
  }

  if (refreshCoordinator.current === null) {
    refreshCoordinator.current = createRefreshCoordinator(loadDashboard);
  }

  async function loadDashboard(): Promise<void> {
    if (!localServiceSetupAttempted.current) {
      localServiceSetupAttempted.current = true;
      try {
        await daemonApi.ensureLocalService();
      } catch {
        // Development and unsupported service managers use the normal status UI.
      }
    }

    const [
      health,
      sourceStatusResponse,
      insightResponse,
      timelineResponse,
      platformResponse,
      settingsResponse,
      dataResponse,
      connectorResponse,
      autostartResponse,
    ] = await Promise.allSettled([
      daemonApi.health(),
      daemonApi.sourceStatus(),
      daemonApi.insights(),
      daemonApi.timeline(),
      daemonApi.platformCapabilities(),
      daemonApi.collectionSettings(),
      daemonApi.localDataSummary(),
      daemonApi.chromeConnectorStatus(),
      daemonApi.localServiceAutostartStatus(),
    ]);

    const serviceReady =
      health.status === "fulfilled" && health.value.type === "health";
    setServiceState(serviceReady ? "ready" : "unavailable");
    if (sourceStatusResponse.status === "fulfilled") {
      const connector =
        connectorResponse.status === "fulfilled"
          ? connectorResponse.value
          : undefined;
      setConnections(
        toConnectionStatusModel(sourceStatusResponse.value, connector),
      );
    } else {
      setConnections({
        state: "unavailable",
        items: [],
        message: "Connection status is unavailable from the local service.",
      });
    }
    if (insightResponse.status === "fulfilled") {
      setInsights(toDailyRhythmDashboardModel(insightResponse.value));
    } else {
      setInsights(
        toDailyRhythmDashboardModel({
          type: "error",
          protocol_version: PROTOCOL_VERSION,
          code: "internal",
        }),
      );
    }
    if (timelineResponse.status === "fulfilled") {
      setTimeline(toDailyTimelineDashboardModel(timelineResponse.value));
    } else {
      setTimeline(
        toDailyTimelineDashboardModel({
          type: "error",
          protocol_version: PROTOCOL_VERSION,
          code: "internal",
        }),
      );
    }
    if (settingsResponse.status === "fulfilled") {
      setCollection(toSignalCollectionControls(settingsResponse.value));
      setCollectionUnavailable(false);
    } else {
      setCollectionUnavailable(true);
    }
    if (platformResponse.status === "fulfilled") {
      setPlatform(toPlatformCapabilityModel(platformResponse.value));
    } else {
      setPlatform(
        toPlatformCapabilityModel({
          type: "error",
          protocol_version: PROTOCOL_VERSION,
          code: "internal",
        }),
      );
    }
    if (dataResponse.status === "fulfilled") {
      setDataControl(toLocalDataControlModel(dataResponse.value));
      setDataUnavailable(false);
    } else {
      setDataUnavailable(true);
    }
    if (autostartResponse.status === "fulfilled") {
      setLocalServiceAutostart(autostartResponse.value);
    } else {
      setLocalServiceAutostart({
        supported: false,
        enabled: false,
        active: serviceReady,
      });
    }
  }

  async function refreshDashboard(): Promise<void> {
    const coordinator = refreshCoordinator.current;
    if (coordinator === null) {
      throw new Error("Dashboard refresh coordinator was not initialized.");
    }
    if (coordinator.isRunning()) {
      return coordinator.run();
    }

    setRefreshing(true);
    try {
      await coordinator.run();
    } finally {
      setRefreshing(false);
    }
  }

  const anyDialogOpen =
    clearConfirmation !== undefined ||
    exportConfirmation !== undefined ||
    backupConfirmation !== undefined ||
    createdBackup !== undefined ||
    restoreBackupOpen ||
    signalDeletion !== undefined ||
    annotationDeletion !== undefined ||
    localRemoval !== undefined ||
    annotationDraftState !== undefined ||
    supportDiagnostics !== undefined;

  useEffect(() => {
    void refreshDashboard();
    void daemonApi
      .appVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion(undefined));
  }, []);

  useEffect(() => {
    if (anyDialogOpen) return;
    const refreshWhenVisible = () => {
      if (shouldAutoRefresh(document.visibilityState)) {
        void refreshDashboard();
      }
    };
    const interval = window.setInterval(
      refreshWhenVisible,
      DASHBOARD_AUTO_REFRESH_INTERVAL_MS,
    );
    document.addEventListener("visibilitychange", refreshWhenVisible);

    return () => {
      window.clearInterval(interval);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
    };
  }, [anyDialogOpen]);

  async function setSignal(signal: SignalId, enabled: boolean): Promise<void> {
    try {
      const response = await daemonApi.setSignalCollection(signal, enabled);
      setCollection(toSignalCollectionControls(response));
      setNotice(enabled ? "Collection enabled locally." : "Collection paused.");
    } catch {
      setNotice("The local service could not update that setting.");
    }
  }

  function openSupportDiagnostics(): void {
    if (appVersion === undefined) {
      setNotice("App version is not available yet.");
      return;
    }
    setSupportDiagnostics(
      createSupportDiagnostics({
        appVersion,
        serviceState,
        connections,
        platform,
        localDataAvailable: dataControl !== undefined && !dataUnavailable,
      }),
    );
  }

  async function enableStarterSet(): Promise<void> {
    let latestControls = collection;
    const result = await enableBrowserStarterSet(collection, async (signal) => {
      const response = await daemonApi.setSignalCollection(signal, true);
      latestControls = toSignalCollectionControls(response);
    });
    setCollection(latestControls);

    if (result.attemptedCount === 0) {
      setNotice("The starter browser set is already enabled.");
    } else if (result.failedSignals.length === 0) {
      setNotice("Starter browser set enabled locally.");
    } else {
      setNotice(
        `${result.enabledCount} of ${result.attemptedCount} remaining starter signals enabled. Review the browser controls and try again.`,
      );
    }
  }

  async function enableOsActivitySet(): Promise<void> {
    let latestControls = collection;
    const result = await enableOsActivityStarterSet(
      collection,
      async (signal) => {
        const response = await daemonApi.setSignalCollection(signal, true);
        latestControls = toSignalCollectionControls(response);
      },
    );
    setCollection(latestControls);

    if (result.attemptedCount === 0) {
      setNotice("Computer activity is already enabled.");
    } else if (result.failedSignals.length === 0) {
      setNotice("Computer activity enabled locally.");
    } else {
      setNotice(
        `${result.enabledCount} of ${result.attemptedCount} remaining computer activity signals enabled. Review Sources and try again.`,
      );
    }
  }

  async function setServiceAutostart(enabled: boolean): Promise<void> {
    setAutostartUpdating(true);
    try {
      const status = await daemonApi.setLocalServiceAutostart(enabled);
      setLocalServiceAutostart(status);
      setNotice(
        enabled
          ? "Local service will start when you sign in."
          : "Local service will start when you open mindcanary.",
      );
    } catch {
      setNotice("The startup setting could not be changed on this device.");
    } finally {
      setAutostartUpdating(false);
    }
  }

  async function submitCheckIn(checkIn: CheckInRecord): Promise<boolean> {
    try {
      const response = await daemonApi.submitCheckIn(checkIn);
      if (response.type !== "check_in_acknowledged") {
        throw new TypeError("Unexpected check-in response.");
      }
      setNotice("Check-in saved on this device.");
      await refreshDashboard();
      return true;
    } catch {
      setNotice("The check-in could not be saved.");
      return false;
    }
  }

  async function prepareSignalDeletion(signal: SignalId): Promise<void> {
    try {
      const response = await daemonApi.prepareDeleteSignalRecords(signal);
      setClearConfirmation(undefined);
      setExportConfirmation(undefined);
      setSignalDeletion(toSignalDeletionConfirmation(response));
    } catch {
      setNotice("The local service could not review that signal's records.");
    }
  }

  async function confirmSignalDeletion(): Promise<void> {
    if (signalDeletion === undefined) {
      return;
    }
    try {
      const response = await daemonApi.deleteSignalRecords(
        signalDeletion.signal,
        signalDeletion.confirmationToken,
      );
      const result = toSignalDeletionResult(response);
      setSignalDeletion(undefined);
      setNotice(`${result.label} history deleted: ${result.summaryText}.`);
      await refreshDashboard();
    } catch {
      setNotice("The confirmation expired. Review that signal again.");
      setSignalDeletion(undefined);
    }
  }

  async function prepareClear(): Promise<void> {
    try {
      const response = await daemonApi.prepareClearLocalRecords();
      const model = toLocalDataControlModel(response);
      if ("confirmationToken" in model) {
        setSignalDeletion(undefined);
        setExportConfirmation(undefined);
        setClearConfirmation(model);
      }
    } catch {
      setNotice("The local service could not prepare record clearing.");
    }
  }

  async function prepareExport(): Promise<void> {
    try {
      const response = await daemonApi.prepareExportLocalRecords();
      const model = toLocalDataControlModel(response);
      if ("confirmationToken" in model) {
        setSignalDeletion(undefined);
        setClearConfirmation(undefined);
        setExportConfirmation(model);
      }
    } catch {
      setNotice("The local service could not prepare local export.");
    }
  }

  async function confirmExport(exportDirectory: string): Promise<void> {
    if (exportConfirmation === undefined) {
      return;
    }
    try {
      const response = await daemonApi.exportLocalRecords(
        exportConfirmation.confirmationToken,
        exportDirectory,
      );
      const model = toLocalDataControlModel(response);
      if (!("reportPath" in model)) {
        throw new TypeError("Unexpected export response.");
      }
      setDataControl(model);
      setExportConfirmation(undefined);
      setNotice(`Local export written: ${model.reportPath}.`);
      await refreshDashboard();
    } catch {
      setNotice("The export could not be written. Review the folder path.");
    }
  }

  async function prepareBackup(): Promise<void> {
    try {
      const response = await daemonApi.prepareCreateLocalBackup();
      setBackupConfirmation(toBackupConfirmationModel(response));
      setClearConfirmation(undefined);
      setExportConfirmation(undefined);
      setSignalDeletion(undefined);
    } catch {
      setNotice("The local service could not prepare an encrypted backup.");
    }
  }

  async function confirmBackup(backupPath: string): Promise<void> {
    if (backupConfirmation === undefined) {
      return;
    }
    try {
      const response = await daemonApi.createLocalBackup(
        backupConfirmation.confirmationToken,
        backupPath,
      );
      setCreatedBackup(toCreatedBackupModel(response));
      setBackupConfirmation(undefined);
    } catch {
      setNotice(
        "The encrypted backup could not be written. Use a new absolute file path in an existing folder.",
      );
    }
  }

  async function verifyBackup(
    backupPath: string,
    recoverySecret: string,
  ): Promise<VerifiedBackupModel> {
    return toVerifiedBackupModel(
      await daemonApi.verifyLocalBackup(backupPath, recoverySecret),
    );
  }

  async function restoreBackup(
    backupPath: string,
    recoverySecret: string,
  ): Promise<RestoredBackupModel> {
    const restored = toRestoredBackupModel(
      await daemonApi.restoreLocalBackup(backupPath, recoverySecret),
    );
    setRestoreBackupOpen(false);
    setNotice(`Encrypted backup restored: ${restored.summaryText}.`);
    await refreshDashboard();
    return restored;
  }

  async function confirmClear(): Promise<void> {
    if (clearConfirmation === undefined) {
      return;
    }
    try {
      const response = await daemonApi.clearLocalRecords(
        clearConfirmation.confirmationToken,
      );
      setDataControl(toLocalDataControlModel(response));
      setClearConfirmation(undefined);
      setInsights(undefined);
      setTimeline(undefined);
      setNotice("Local records cleared.");
      await refreshDashboard();
    } catch {
      setNotice("The confirmation expired. Review the records again.");
      setClearConfirmation(undefined);
    }
  }

  async function prepareAnnotationDeletion(
    annotationId: string,
  ): Promise<void> {
    try {
      const response = await daemonApi.prepareDeleteAnnotation(annotationId);
      setAnnotationDeletion(toAnnotationDeletionConfirmation(response));
    } catch {
      setNotice("The note could not be reviewed for deletion.");
    }
  }

  async function confirmAnnotationDeletion(): Promise<void> {
    if (annotationDeletion === undefined) {
      return;
    }
    try {
      await daemonApi.deleteAnnotation(
        annotationDeletion.annotationId,
        annotationDeletion.confirmationToken,
      );
      setAnnotationDeletion(undefined);
      setNotice("Note deleted.");
      await refreshDashboard();
    } catch {
      setNotice("The confirmation expired. Try again.");
      setAnnotationDeletion(undefined);
    }
  }

  async function confirmLocalRemoval(
    confirmationPhrase: string,
  ): Promise<void> {
    try {
      const report = await daemonApi.completeLocalRemoval(confirmationPhrase);
      setLocalRemoval(undefined);
      setClearConfirmation(undefined);
      setExportConfirmation(undefined);
      setBackupConfirmation(undefined);
      setCreatedBackup(undefined);
      setRestoreBackupOpen(false);
      setSignalDeletion(undefined);
      setDataControl(undefined);
      setInsights(undefined);
      setTimeline(undefined);
      setConnections(undefined);
      setServiceState("unavailable");
      setNotice(`Local removal completed: ${localRemovalResultText(report)}.`);
    } catch {
      setNotice(
        "Local removal was not completed. Check the confirmation phrase.",
      );
    }
  }

  async function connectChrome(): Promise<void> {
    setConnectorConnecting(true);
    try {
      await daemonApi.connectChrome();
      await refreshDashboard();
    } catch {
      setNotice(
        "Could not connect Chrome. The native host helper might be missing.",
      );
    } finally {
      setConnectorConnecting(false);
    }
  }

  const setup = toSetupChecklistModel({
    serviceState,
    collection,
    collectionUnavailable,
    hasLocalRecords:
      dataControl === undefined ? undefined : dataControl.isEmpty === false,
    localDataUnavailable: dataUnavailable,
  });
  const onboarding = toOnboardingModel({
    completedLocally: onboardingCompleted,
    setup,
    serviceState,
  });
  const browserConnection = browserConnectionItem(connections);
  const chromeActionLabel =
    browserConnection?.action?.type === "connect_chrome"
      ? "Connect Chrome context"
      : "Enable Chrome context";
  const chromeActionDetail =
    browserConnection?.action?.type === "setup_command"
      ? "Development builds may still need the setup command shown in Sources."
      : "Uses the starter browser aggregates. No URLs, titles, page text, or history.";
  const computerActivityAvailable = isComputerActivityAvailable(platform);
  const setupNeedsAttention =
    serviceState === "unavailable" ||
    (serviceState === "ready" && (collectionUnavailable || dataUnavailable));

  function completeOnboarding(section: AppSection = "today"): void {
    setOnboardingCompleted(true);
    setActiveSection(section);
    try {
      window.localStorage.setItem(ONBOARDING_STORAGE_KEY, "true");
    } catch {
      // Local storage can be unavailable in unusual webviews; dismissal remains
      // valid for this app session.
    }
  }

  return (
    <div className="app-shell">
      <Header
        activeSection={activeSection}
        onSelectSection={setActiveSection}
        serviceState={serviceState}
        showNavigation={!onboarding.show}
      />

      <main className="dashboard">
        {notice !== undefined && (
          <div className="notice" role="status" aria-live="polite">
            {notice}
            <button
              className="quiet-button notice-dismiss"
              aria-label="Dismiss notice"
              onClick={() => setNotice(undefined)}
              type="button"
            >
              Dismiss
            </button>
          </div>
        )}

        {onboarding.show ? (
          <OnboardingFlow
            browserActionDetail={chromeActionDetail}
            browserActionLabel={chromeActionLabel}
            computerActivityAvailable={computerActivityAvailable}
            localServiceAutostart={localServiceAutostart}
            model={onboarding}
            onComplete={completeOnboarding}
            onEnableBrowserContext={async () => {
              if (browserConnection?.action?.type === "connect_chrome") {
                await connectChrome();
              }
              await enableStarterSet();
            }}
            onEnableComputerActivity={enableOsActivitySet}
            onSetLocalServiceAutostart={setServiceAutostart}
            autostartUpdating={autostartUpdating}
          />
        ) : (
          <>
            {setupNeedsAttention && (
              <SetupChecklist
                model={setup}
                onRefresh={refreshDashboard}
                refreshing={refreshing}
              />
            )}

            <SectionTabs
              activeSection={activeSection}
              onSelectSection={setActiveSection}
            />

            {activeSection === "today" && (
              <div className="dashboard-grid">
                <div className="primary-column">
                  <CheckInPanel onSubmit={submitCheckIn} timeline={timeline} />
                  <button
                    className="annotation-launcher"
                    onClick={() =>
                      setAnnotationDraftState(emptyAnnotationDraft())
                    }
                    type="button"
                  >
                    <span>
                      <strong>Add private context</strong>
                      <small>
                        Note something that may help you understand today later.
                      </small>
                    </span>
                    <span aria-hidden="true">Add note</span>
                  </button>
                  <LatestLocalRecordPanel
                    model={toLatestLocalRecordModel(timeline)}
                    onOpenHistory={() => setActiveSection("history")}
                  />
                </div>
                <aside className="side-column">
                  <ConnectionsPanel
                    model={connections}
                    serviceReady={serviceState === "ready"}
                    onConnectChrome={connectChrome}
                    connectorConnecting={connectorConnecting}
                  />
                </aside>
              </div>
            )}

            {activeSection === "history" && (
              <div className="single-column">
                <InsightsPanel model={insights} />
                <TimelinePanel
                  model={timeline}
                  onAddAnnotation={(date) =>
                    setAnnotationDraftState(emptyAnnotationDraft(date))
                  }
                  onEditAnnotation={(record) => {
                    setAnnotationDraftState(annotationDraft(record));
                  }}
                  onDeleteAnnotation={(annotationId) =>
                    void prepareAnnotationDeletion(annotationId)
                  }
                />
              </div>
            )}

            {activeSection === "sources" && (
              <div className="dashboard-grid">
                <div className="primary-column">
                  <ConnectionsPanel
                    model={connections}
                    serviceReady={serviceState === "ready"}
                    onConnectChrome={connectChrome}
                    connectorConnecting={connectorConnecting}
                  />
                  <CollectionPanel
                    controls={collection}
                    unavailable={collectionUnavailable}
                    onChange={setSignal}
                    onEnableStarterSet={enableStarterSet}
                    onEnableOsActivitySet={enableOsActivitySet}
                    onPrepareDelete={prepareSignalDeletion}
                  />
                </div>
                <aside className="side-column">
                  <ServiceStartupPanel
                    status={localServiceAutostart}
                    updating={autostartUpdating}
                    onSetAutostart={setServiceAutostart}
                  />
                  <PlatformPanel model={platform} />
                </aside>
              </div>
            )}

            {activeSection === "data" && (
              <div className="single-column narrow-column">
                <DataPanel
                  model={dataControl}
                  unavailable={dataUnavailable}
                  onPrepareClear={prepareClear}
                  onPrepareExport={prepareExport}
                  onPrepareBackup={prepareBackup}
                  onOpenRestore={() => setRestoreBackupOpen(true)}
                  onPrepareRemoval={() => setLocalRemoval(localRemovalModel())}
                />
                <SupportPanel
                  appVersion={appVersion}
                  onPreview={openSupportDiagnostics}
                />
              </div>
            )}
          </>
        )}
      </main>

      {clearConfirmation !== undefined && (
        <ClearRecordsDialog
          model={clearConfirmation}
          onCancel={() => setClearConfirmation(undefined)}
          onConfirm={confirmClear}
        />
      )}
      {exportConfirmation !== undefined && (
        <ExportRecordsDialog
          model={exportConfirmation}
          onCancel={() => setExportConfirmation(undefined)}
          onConfirm={confirmExport}
        />
      )}
      {backupConfirmation !== undefined && (
        <CreateBackupDialog
          model={backupConfirmation}
          onCancel={() => setBackupConfirmation(undefined)}
          onConfirm={confirmBackup}
        />
      )}
      {createdBackup !== undefined && (
        <BackupCreatedDialog
          model={createdBackup}
          onClose={() => setCreatedBackup(undefined)}
        />
      )}
      {restoreBackupOpen && (
        <RestoreBackupDialog
          profileEmpty={dataControl?.isEmpty === true}
          onCancel={() => setRestoreBackupOpen(false)}
          onRestore={restoreBackup}
          onVerify={verifyBackup}
        />
      )}
      {signalDeletion !== undefined && (
        <DeleteSignalRecordsDialog
          model={signalDeletion}
          onCancel={() => setSignalDeletion(undefined)}
          onConfirm={confirmSignalDeletion}
        />
      )}
      {annotationDeletion !== undefined && (
        <AnnotationDeleteDialog
          model={annotationDeletion}
          onCancel={() => setAnnotationDeletion(undefined)}
          onConfirm={confirmAnnotationDeletion}
        />
      )}
      {localRemoval !== undefined && (
        <LocalRemovalDialog
          model={localRemoval}
          onCancel={() => setLocalRemoval(undefined)}
          onConfirm={confirmLocalRemoval}
        />
      )}
      {annotationDraftState !== undefined && (
        <AnnotationDialog
          initialDraft={annotationDraftState}
          onClose={() => setAnnotationDraftState(undefined)}
          onSave={submitAnnotation}
        />
      )}
      {supportDiagnostics !== undefined && (
        <SupportDiagnosticsDialog
          model={supportDiagnostics}
          onClose={() => setSupportDiagnostics(undefined)}
          onCopied={() =>
            setNotice(
              "Support information copied. Nothing was sent automatically.",
            )
          }
        />
      )}
    </div>
  );
}

function readOnboardingCompleted(): boolean {
  try {
    return window.localStorage.getItem(ONBOARDING_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function browserConnectionItem(
  model?: ConnectionStatusModel,
): ConnectionStatusItemModel | undefined {
  return model?.items.find((item) => item.id === "browser");
}

function isComputerActivityAvailable(model?: PlatformCapabilityModel): boolean {
  return (
    model?.state === "ready" &&
    model.capabilities.some(isAvailableComputerActivityCapability)
  );
}

function isAvailableComputerActivityCapability(
  capability: PlatformCapabilityCardModel,
): boolean {
  return (
    capability.id === "os_active_idle_duration" &&
    capability.status === "available"
  );
}

function TagSelector({
  selectedTags,
  onToggleTag,
}: {
  selectedTags: ContextTag[];
  onToggleTag: (tag: ContextTag) => void;
}) {
  return (
    <div className="tag-list">
      {contextTagOptions().map((tag) => {
        const selected = selectedTags.includes(tag);
        return (
          <button
            aria-pressed={selected}
            className="tag-button"
            data-selected={selected}
            key={tag}
            onClick={() => onToggleTag(tag)}
            type="button"
          >
            {CONTEXT_TAG_LABELS[tag]}
          </button>
        );
      })}
    </div>
  );
}

function SectionTabs({
  activeSection,
  onSelectSection,
}: {
  activeSection: AppSection;
  onSelectSection: (section: AppSection) => void;
}) {
  const sections: { id: AppSection; label: string; detail: string }[] = [
    {
      id: "today",
      label: "Today",
      detail: "Check in and recent changes",
    },
    {
      id: "history",
      label: "History",
      detail: "Daily rhythm calendar",
    },
    {
      id: "sources",
      label: "Sources",
      detail: "Optional local connectors",
    },
    {
      id: "data",
      label: "Data",
      detail: "Export and deletion",
    },
  ];

  return (
    <nav className="section-tabs" aria-label="mindcanary sections">
      {sections.map((section) => (
        <button
          aria-current={activeSection === section.id ? "page" : undefined}
          key={section.id}
          onClick={() => onSelectSection(section.id)}
          type="button"
        >
          <strong>{section.label}</strong>
          <span>{section.detail}</span>
        </button>
      ))}
    </nav>
  );
}

function OnboardingFlow({
  browserActionDetail,
  browserActionLabel,
  computerActivityAvailable,
  localServiceAutostart,
  model,
  onComplete,
  onEnableBrowserContext,
  onEnableComputerActivity,
  onSetLocalServiceAutostart,
  autostartUpdating,
}: {
  browserActionDetail: string;
  browserActionLabel: string;
  computerActivityAvailable: boolean;
  localServiceAutostart?: LocalServiceAutostartStatus;
  model: ReturnType<typeof toOnboardingModel>;
  onComplete: (section?: AppSection) => void;
  onEnableBrowserContext: () => Promise<void>;
  onEnableComputerActivity: () => Promise<void>;
  onSetLocalServiceAutostart: (enabled: boolean) => Promise<void>;
  autostartUpdating: boolean;
}) {
  const [step, setStep] = useState<OnboardingStep>("intro");
  const [pendingAction, setPendingAction] = useState<
    "startup" | "browser" | "computer" | undefined
  >();
  const stepIndex = { intro: 1, browser: 2, computer: 3, startup: 4 }[step];

  async function runConnectorAction(
    action: () => Promise<void>,
    pending: "startup" | "browser" | "computer",
    after: () => void,
  ): Promise<void> {
    setPendingAction(pending);
    try {
      await action();
      after();
    } finally {
      setPendingAction(undefined);
    }
  }

  return (
    <section className="onboarding-shell" aria-labelledby="onboarding-title">
      <div className="onboarding-card">
        <div className="onboarding-progress">
          <span>Step {stepIndex} of 4</span>
          <div aria-hidden="true">
            {(
              ["intro", "browser", "computer", "startup"] as OnboardingStep[]
            ).map((candidate) => (
              <span data-active={candidate === step} key={candidate} />
            ))}
          </div>
        </div>

        {step === "intro" && (
          <div className="onboarding-step">
            <p className="eyebrow">Welcome</p>
            <h1 id="onboarding-title">Your routines, kept private.</h1>
            <p>
              <span className="brand-highlight">mindcanary</span> helps you
              notice how your rhythms change over time. Everything stays on your
              device unless you decide otherwise.
            </p>
            <div className="onboarding-principles">
              <div className="onboarding-principles-content">
                <strong>Local first</strong>
                <span>No account, no cloud, no telemetry.</span>
              </div>
              <div className="onboarding-principles-content">
                <strong>You pick what to track</strong>
                <span>
                  Check-ins work on their own. Browser and computer context are
                  always optional.
                </span>
              </div>
              <div className="onboarding-principles-content">
                <strong>Patterns, not labels</strong>
                <span>You decide what the data means.</span>
              </div>
            </div>
            <div className="onboarding-actions">
              <button
                className="primary-button"
                onClick={() => setStep("browser")}
                type="button"
              >
                {model.primaryActionLabel}
              </button>
            </div>
          </div>
        )}

        {step === "browser" && (
          <div className="onboarding-step">
            <p className="eyebrow">Optional browser context</p>
            <h1 id="onboarding-title">Want browser context?</h1>
            <p>
              mindcanary works with check-ins alone. Chrome adds browsing rhythm
              summaries - never the pages you visit.
            </p>
            <div className="onboarding-choice-grid">
              <button
                className="onboarding-choice"
                disabled={pendingAction !== undefined}
                onClick={() =>
                  void runConnectorAction(
                    onEnableBrowserContext,
                    "browser",
                    () => setStep("computer"),
                  )
                }
                type="button"
              >
                <strong>
                  {pendingAction === "browser"
                    ? "Setting up Chrome..."
                    : browserActionLabel}
                </strong>
                <span>{browserActionDetail}</span>
              </button>
              <button
                className="onboarding-choice"
                disabled={pendingAction !== undefined}
                onClick={() => setStep("computer")}
                type="button"
              >
                <strong>Skip Chrome for now</strong>
                <span>You can add browser context later from Sources.</span>
              </button>
            </div>
            <div className="onboarding-actions">
              <button
                className="secondary-button"
                onClick={() => setStep("intro")}
                disabled={pendingAction !== undefined}
                type="button"
              >
                Back
              </button>
            </div>
          </div>
        )}

        {step === "computer" && (
          <div className="onboarding-step">
            <p className="eyebrow">Optional computer activity</p>
            <h1 id="onboarding-title">Track computer activity?</h1>
            <p>
              This records local active and idle duration totals. It does not
              store app names, window titles, document names, or content.
            </p>
            <div className="onboarding-choice-grid">
              <button
                className="onboarding-choice"
                disabled={
                  pendingAction !== undefined || !computerActivityAvailable
                }
                onClick={() =>
                  void runConnectorAction(
                    onEnableComputerActivity,
                    "computer",
                    () => setStep("startup"),
                  )
                }
                type="button"
              >
                <strong>
                  {pendingAction === "computer"
                    ? "Enabling computer activity..."
                    : computerActivityAvailable
                      ? "Enable computer activity"
                      : "Not available on this device"}
                </strong>
                <span>
                  {computerActivityAvailable
                    ? "Uses active and idle duration only."
                    : "You can keep using check-ins and browser context."}
                </span>
              </button>
              <button
                className="onboarding-choice"
                disabled={pendingAction !== undefined}
                onClick={() => setStep("startup")}
                type="button"
              >
                <strong>Skip computer activity for now</strong>
                <span>You can add supported device context later.</span>
              </button>
            </div>
            <div className="onboarding-actions">
              <button
                className="secondary-button"
                disabled={pendingAction !== undefined}
                onClick={() => setStep("browser")}
                type="button"
              >
                Back
              </button>
            </div>
          </div>
        )}

        {step === "startup" && (
          <div className="onboarding-step">
            <p className="eyebrow">Startup</p>
            <h1 id="onboarding-title">Start mindcanary when you sign in?</h1>
            <p>
              This controls whether the local service resumes after a reboot.
              You can change it later from Sources.
            </p>
            <div className="onboarding-choice-grid">
              {localServiceAutostart === undefined ? (
                <button className="onboarding-choice" disabled type="button">
                  <strong>Checking startup setting...</strong>
                  <span>
                    mindcanary is checking whether this build can manage login
                    startup.
                  </span>
                </button>
              ) : localServiceAutostart.supported === false ? (
                <button
                  className="onboarding-choice"
                  disabled={pendingAction !== undefined || autostartUpdating}
                  onClick={() => onComplete("today")}
                  type="button"
                >
                  <strong>Continue</strong>
                  <span>
                    Login startup is managed by the installed Linux app. This
                    development profile can still be tested normally.
                  </span>
                </button>
              ) : (
                <>
                  <button
                    className="onboarding-choice"
                    disabled={pendingAction !== undefined || autostartUpdating}
                    onClick={() =>
                      void runConnectorAction(
                        () => onSetLocalServiceAutostart(true),
                        "startup",
                        () => onComplete("today"),
                      )
                    }
                    type="button"
                  >
                    <strong>
                      {pendingAction === "startup" || autostartUpdating
                        ? "Saving..."
                        : "Start at login"}
                    </strong>
                    <span>
                      Best if you want optional local sources to keep working
                      after a reboot.
                    </span>
                  </button>
                  <button
                    className="onboarding-choice"
                    disabled={pendingAction !== undefined || autostartUpdating}
                    onClick={() =>
                      void runConnectorAction(
                        () => onSetLocalServiceAutostart(false),
                        "startup",
                        () => onComplete("today"),
                      )
                    }
                    type="button"
                  >
                    <strong>Only when I open mindcanary</strong>
                    <span>
                      More manual, but nothing starts at login unless you change
                      it later.
                    </span>
                  </button>
                </>
              )}
            </div>
            <div className="onboarding-actions">
              <button
                className="secondary-button"
                disabled={pendingAction !== undefined || autostartUpdating}
                onClick={() => setStep("computer")}
                type="button"
              >
                Back
              </button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

function ConnectionsPanel({
  model,
  serviceReady,
  onConnectChrome,
  connectorConnecting,
}: {
  model?: ConnectionStatusModel;
  serviceReady: boolean;
  onConnectChrome: () => Promise<void>;
  connectorConnecting: boolean;
}) {
  return (
    <Panel
      compact
      eyebrow="Connections"
      title="Local sources"
      description="Shows which optional sources are connected on this device."
    >
      {model === undefined && <PanelLoading text="Checking local sources..." />}
      {model?.state === "unavailable" && !serviceReady && (
        <div className="connection-list">
          <ConnectionRow item={daemonConnectionItem(false)} />
        </div>
      )}
      {model?.state === "unavailable" && serviceReady && (
        <EmptyState
          title="Source status unavailable"
          body={
            model.message ?? "The local service did not return source status."
          }
        />
      )}
      {model?.state === "ready" && (
        <div className="connection-list">
          <ConnectionRow item={daemonConnectionItem(serviceReady)} />
          {model.items.map((item) => (
            <ConnectionRow
              item={item}
              key={item.id}
              onConnectChrome={onConnectChrome}
              connectorConnecting={connectorConnecting}
            />
          ))}
        </div>
      )}
    </Panel>
  );
}

function ConnectionRow({
  item,
  onConnectChrome,
  connectorConnecting,
}: {
  item: ConnectionStatusItemModel;
  onConnectChrome?: () => Promise<void>;
  connectorConnecting?: boolean;
}) {
  return (
    <div className="connection-row">
      <span
        className="connection-indicator"
        data-tone={item.tone}
        aria-hidden="true"
      />
      <div>
        <strong>{item.label}</strong>
        <span>{item.detail}</span>
        {item.action?.type === "connect_chrome" && onConnectChrome && (
          <div style={{ marginTop: "8px" }}>
            <button
              className="secondary-button"
              disabled={connectorConnecting}
              onClick={() => void onConnectChrome()}
              type="button"
            >
              {connectorConnecting ? "Connecting..." : "Connect Chrome"}
            </button>
          </div>
        )}
        {item.action?.type === "setup_command" && item.action.command && (
          <details className="developer-command">
            <summary>Development setup command</summary>
            <pre>{item.action.command}</pre>
          </details>
        )}
      </div>
      <span className="connection-state" data-tone={item.tone}>
        {item.statusLabel}
      </span>
    </div>
  );
}

function ServiceStartupPanel({
  status,
  updating,
  onSetAutostart,
}: {
  status?: LocalServiceAutostartStatus;
  updating: boolean;
  onSetAutostart: (enabled: boolean) => Promise<void>;
}) {
  return (
    <Panel
      compact
      eyebrow="Startup"
      title="Local service"
      description="Controls whether the private local service starts when you sign in."
    >
      {status === undefined && (
        <PanelLoading text="Checking startup setting..." />
      )}
      {status?.supported === false && (
        <EmptyState
          title="Packaged app only"
          body="Login startup is managed by the installed Linux app. Development profiles can still use whichever local service you start for testing."
        />
      )}
      {status?.supported === true && (
        <div className="control-list">
          <div className="control-row">
            <div className="control-copy">
              <strong>Start at login</strong>
              <span>
                Optional. Keep it on if you want local sources to resume after
                a reboot.
              </span>
            </div>
            <div className="control-actions">
              <span className="connection-state" data-tone="neutral">
                {status.enabled ? "On" : "Off"}
              </span>
              <button
                role="switch"
                aria-checked={status.enabled}
                aria-label="Start local service at login"
                className="switch"
                data-enabled={status.enabled}
                disabled={updating}
                onClick={() => void onSetAutostart(!status.enabled)}
                title={
                  status.enabled
                    ? "Disable login startup"
                    : "Enable login startup"
                }
                type="button"
              >
                <span />
              </button>
            </div>
          </div>
        </div>
      )}
    </Panel>
  );
}

function PlatformPanel({ model }: { model?: PlatformCapabilityModel }) {
  return (
    <Panel
      compact
      eyebrow="Device support"
      title="Device capabilities"
      description="Shows which activity signals this device supports."
    >
      {model === undefined && (
        <PanelLoading text="Checking device support..." />
      )}
      {model?.state === "unavailable" && (
        <EmptyState title="Device support unavailable" body={model.message} />
      )}
      {model?.state === "ready" && (
        <>
          <div className="platform-summary">
            <strong>{model.environmentText}</strong>
            <span>{model.coverageText}</span>
          </div>
          <div className="capability-list">
            {model.capabilities.map((capability) => (
              <div className="capability-row" key={capability.id}>
                <div>
                  <strong>{capability.label}</strong>
                  <span>{capability.detail}</span>
                </div>
                <span data-status={capability.status}>
                  {capability.statusLabel}
                </span>
              </div>
            ))}
          </div>
        </>
      )}
    </Panel>
  );
}

function TimelinePanel({
  model,
  onAddAnnotation,
  onEditAnnotation,
  onDeleteAnnotation,
}: {
  model?: DailyTimelineDashboardModel;
  onAddAnnotation: (date: string) => void;
  onEditAnnotation: (record: AnnotationRecord) => void;
  onDeleteAnnotation: (annotationId: string) => void;
}) {
  return (
    <Panel
      eyebrow="Daily history"
      title="Local rhythms"
      description="A calendar view of what was recorded. Days without data stay visible and are never counted as zero."
    >
      {model === undefined && <PanelLoading text="Loading local history..." />}
      {model?.state === "unavailable" && (
        <EmptyState title="Daily history unavailable" body={model.message} />
      )}
      {model?.state === "empty" && (
        <EmptyState title={model.emptyTitle} body={model.emptyBody} />
      )}
      {model?.state === "ready" && (
        <>
          <div className="coverage-row">
            <span>{model.coverageText}</span>
            {model.isTruncated && <span>Showing the most recent days</span>}
          </div>
          <div className="timeline-chart-grid">
            <MetricChart
              days={model.days}
              emptyLabel="No average-tab records in this range yet."
              formatValue={(value) => `${formatCompact(value)} tabs`}
              source={BROWSER_TIMELINE_SOURCE}
              title="Average open tabs"
              value={(day) => day.browser?.openTabs ?? null}
              zeroLabel="Average open tabs were recorded as 0 in this range."
            />
            <MetricChart
              days={model.days}
              emptyLabel="No tab-switch records in this range yet."
              formatValue={(value) => `${formatCompact(value)} switches`}
              source={BROWSER_TIMELINE_SOURCE}
              title="Tab switching"
              value={(day) => day.browser?.tabSwitches ?? null}
              zeroLabel="No tab switches recorded in this range."
            />
            <MetricChart
              days={model.days}
              emptyLabel="No retained-tab records yet. This appears after Chrome observes a local day boundary."
              formatValue={(value) => `${formatCompact(value)} tabs`}
              source={BROWSER_TIMELINE_SOURCE}
              title="Tabs retained across days"
              value={(day) => day.browser?.retainedAcrossDays ?? null}
              zeroLabel="No tabs were retained across day boundaries in this range."
            />
            {model.days.some(
              (day) => day.browser?.continuousScrollingMinutes != null,
            ) && (
              <MetricChart
                days={model.days}
                emptyLabel="No continuous-scrolling records in this range yet."
                formatValue={(value) => `${formatCompact(value)} min`}
                source="Optional feed adapter · aggregate duration only · no route or content stored"
                title="Continuous scrolling time"
                value={(day) => day.browser?.continuousScrollingMinutes ?? null}
                zeroLabel="Continuous scrolling time was recorded as 0 in this range."
              />
            )}
            <MetricChart
              days={model.days}
              emptyLabel="No OS active-time records in this range yet."
              formatValue={(value) => `${formatCompact(value)} min`}
              source={OS_TIMELINE_SOURCE}
              title="Computer active time"
              value={(day) => day.os?.activeMinutes ?? null}
              zeroLabel="Computer active time was recorded as 0 in this range."
            />
            <MetricChart
              days={model.days}
              emptyLabel="No energy check-ins in this range yet."
              fixedMaximum={7}
              formatValue={(value) => `${formatCompact(value)} of 7`}
              source={CHECK_IN_TIMELINE_SOURCE}
              title="Energy check-in"
              value={(day) => day.checkIn?.energy ?? null}
              zeroLabel="Energy check-ins were recorded as 0 in this range."
            />
          </div>
          <ol className="timeline-day-list">
            {[...model.days].reverse().map((day) => (
              <li
                className="timeline-day"
                data-gap={
                  day.browser === undefined &&
                  day.os === undefined &&
                  day.checkIn === undefined &&
                  day.annotations.length === 0
                }
                key={day.localDate}
              >
                <div className="timeline-day-heading">
                  <time dateTime={day.localDate}>{day.dateLabel}</time>
                  <div className="timeline-day-actions">
                    <span>{day.coverageLabel}</span>
                    <button
                      className="quiet-button timeline-add-annotation"
                      type="button"
                      onClick={() => onAddAnnotation(day.localDate)}
                    >
                      Add note
                    </button>
                  </div>
                </div>
                {day.browser === undefined &&
                day.os === undefined &&
                day.checkIn === undefined &&
                day.annotations.length === 0 ? (
                  <p>
                    No local aggregate, check-in, or annotation was recorded.
                  </p>
                ) : (
                  <div className="timeline-day-details">
                    {day.browser !== undefined && (
                      <div>
                        <strong>Browser</strong>
                        <span>{day.browser.summary}</span>
                        <small>
                          {day.browser.recordedPeriodCount} recorded{" "}
                          {day.browser.recordedPeriodCount === 1
                            ? "period"
                            : "periods"}
                        </small>
                      </div>
                    )}
                    {day.os !== undefined && (
                      <div>
                        <strong>OS</strong>
                        <span>{day.os.summary}</span>
                        <small>
                          {day.os.recordedPeriodCount} recorded{" "}
                          {day.os.recordedPeriodCount === 1
                            ? "period"
                            : "periods"}
                        </small>
                      </div>
                    )}
                    {day.checkIn !== undefined && (
                      <div>
                        <strong>Check-in</strong>
                        <span>{day.checkIn.summary}</span>
                        {day.checkIn.contextLabels.length > 0 && (
                          <small>
                            Context: {day.checkIn.contextLabels.join(", ")}
                          </small>
                        )}
                      </div>
                    )}
                    {day.annotations.map((annotation) => (
                      <div
                        className="timeline-annotation"
                        key={annotation.annotationId}
                      >
                        <strong>Note · {annotation.windowLabel}</strong>
                        <span>{annotation.text}</span>
                        {annotation.contextLabels.length > 0 && (
                          <small>
                            Context: {annotation.contextLabels.join(", ")}
                          </small>
                        )}
                        <div className="timeline-annotation-actions">
                          <button
                            className="quiet-button"
                            onClick={() => onEditAnnotation(annotation.record)}
                            type="button"
                          >
                            Edit
                          </button>
                          <button
                            className="quiet-button danger-button"
                            onClick={() =>
                              onDeleteAnnotation(annotation.annotationId)
                            }
                            type="button"
                          >
                            Delete
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </li>
            ))}
          </ol>
        </>
      )}
    </Panel>
  );
}

function MetricChart({
  days,
  title,
  source,
  value,
  formatValue,
  fixedMaximum,
  emptyLabel,
  zeroLabel,
}: {
  days: TimelineDayModel[];
  title: string;
  source: string;
  value: (day: TimelineDayModel) => number | null;
  formatValue: (value: number) => string;
  fixedMaximum?: number;
  emptyLabel?: string;
  zeroLabel?: string;
}) {
  const values = days.map(value);
  const recordedValues = values.filter(
    (candidate): candidate is number => candidate !== null,
  );
  const hasRecordedValues = recordedValues.length > 0;
  const hasPositiveValues = recordedValues.some((candidate) => candidate > 0);
  const maximum =
    fixedMaximum ??
    Math.max(
      1,
      ...values.filter((candidate): candidate is number => candidate !== null),
    );

  return (
    <figure className="metric-chart">
      <figcaption>
        <strong>{title}</strong>
        <span>{source}</span>
      </figcaption>
      {hasPositiveValues ? (
        <div className="metric-bars" aria-hidden="true">
          {days.map((day, index) => {
            const metric = values[index] ?? null;
            const height =
              metric === null || metric <= 0
                ? 0
                : Math.max(3, Math.min(100, (metric / maximum) * 100));
            return (
              <div
                className="metric-bar-column"
                data-missing={metric === null}
                key={day.localDate}
                title={
                  metric === null
                    ? `${day.localDate}: no record`
                    : `${day.localDate}: ${formatValue(metric)}`
                }
              >
                <span
                  className="metric-bar"
                  style={
                    {
                      "--bar-height": `${height}%`,
                    } as CSSProperties
                  }
                />
                <time>{day.localDate.slice(5)}</time>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="metric-empty-state">
          <span>
            {hasRecordedValues
              ? (zeroLabel ?? "Recorded as 0 in this range.")
              : (emptyLabel ?? "No records in this range yet.")}
          </span>
        </div>
      )}
    </figure>
  );
}

function formatCompact(value: number): string {
  return Math.abs(value - Math.round(value)) < 0.05
    ? String(Math.round(value))
    : value.toFixed(1);
}

function Header({
  activeSection,
  serviceState,
  showNavigation,
  onSelectSection,
}: {
  activeSection: AppSection;
  serviceState: LocalServiceState;
  showNavigation: boolean;
  onSelectSection: (section: AppSection) => void;
}) {
  const status = {
    checking: "Checking local service",
    ready: "Local service ready",
    unavailable: "Local service unavailable",
  }[serviceState];

  return (
    <header className="topbar">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true">
          <img alt="" src="/mindcanary-mark.svg" />
        </span>
        <span>mindcanary</span>
      </div>
      {showNavigation && (
        <nav className="topbar-nav" aria-label="Quick sections">
          {(["today", "history", "sources", "data"] as AppSection[]).map(
            (section) => (
              <button
                aria-current={activeSection === section ? "page" : undefined}
                key={section}
                onClick={() => onSelectSection(section)}
                type="button"
              >
                {sectionLabels[section]}
              </button>
            ),
          )}
        </nav>
      )}
      <div className="service-status" data-state={serviceState}>
        <span className="status-dot" aria-hidden="true" />
        {status}
      </div>
    </header>
  );
}

const sectionLabels: Record<AppSection, string> = {
  today: "Today",
  history: "History",
  sources: "Sources",
  data: "Data",
};

function SetupChecklist({
  model,
  refreshing,
  onRefresh,
}: {
  model: SetupChecklistModel;
  refreshing: boolean;
  onRefresh: () => Promise<void>;
}) {
  return (
    <section
      className="setup-checklist"
      data-complete={model.complete}
      aria-labelledby="setup-checklist-title"
    >
      <div className="setup-checklist-heading">
        <div>
          <p className="eyebrow">Getting started</p>
          <h2 id="setup-checklist-title">{model.title}</h2>
          <p>{model.description}</p>
        </div>
        <div className="setup-checklist-actions">
          <strong>{model.progressText}</strong>
          <button
            className="secondary-button"
            disabled={refreshing}
            onClick={() => void onRefresh()}
            type="button"
          >
            {refreshing ? "Refreshing..." : "Refresh local status"}
          </button>
          <span>Updates automatically while this window is visible.</span>
        </div>
      </div>
      <ol className="setup-step-list">
        {model.steps.map((step, index) => (
          <li data-state={step.state} key={step.id}>
            <span className="setup-step-number" aria-hidden="true">
              {index + 1}
            </span>
            <div>
              <div className="setup-step-title">
                <strong>{step.title}</strong>
                <span>{step.statusLabel}</span>
              </div>
              <p>{step.detail}</p>
              {step.actionLabel !== undefined &&
                step.actionTarget !== undefined && (
                  <a href={step.actionTarget}>{step.actionLabel}</a>
                )}
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}

function CheckInPanel({
  onSubmit,
  timeline,
}: {
  onSubmit: (checkIn: CheckInRecord) => Promise<boolean>;
  timeline?: DailyTimelineDashboardModel;
}) {
  const [draft, setDraft] = useState<CheckInDraft>(EMPTY_CHECK_IN_DRAFT);
  const [submittedLocalDate, setSubmittedLocalDate] = useState<string>();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();

  function updateScale(field: keyof CheckInDraft, raw: string): void {
    setDraft((current) => ({
      ...current,
      [field]: raw === "" ? undefined : Number(raw),
    }));
  }

  function updateOptionalBoolean(
    field: "medicationTaken" | "substanceUse",
    raw: string,
  ): void {
    setDraft((current) => ({
      ...current,
      [field]: raw === "" ? undefined : raw === "yes",
    }));
  }

  function toggleTag(tag: ContextTag): void {
    try {
      setDraft((current) => ({
        ...current,
        contextTags: toggleContextTag(current.contextTags, tag),
      }));
      setError(undefined);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Too many tags.");
    }
  }

  async function save(event: FormEvent): Promise<void> {
    event.preventDefault();
    setError(undefined);
    setSaving(true);
    try {
      const request = createSubmitCheckInRequest(draft);
      if (request.type !== "submit_check_in") {
        throw new TypeError("Unexpected check-in request.");
      }
      if (await onSubmit(request.check_in)) {
        setSubmittedLocalDate(request.check_in.local_date);
        setDraft(EMPTY_CHECK_IN_DRAFT);
      }
    } catch (caught) {
      setError(
        caught instanceof Error ? caught.message : "Check the entered values.",
      );
    } finally {
      setSaving(false);
    }
  }

  const prior = submittedLocalDate
    ? toPriorCheckInReferences(timeline, submittedLocalDate)
    : {};

  return (
    <Panel
      id="check-in-panel"
      eyebrow="Optional check-in"
      title="How has today felt?"
      description="Skip anything you don't want to answer. Each check-in is saved separately."
    >
      <form className="check-in-form" onSubmit={save}>
        <div className="field-grid field-grid-core">
          <label className="field">
            <span>Sleep</span>
            <div className="input-with-unit">
              <input
                min="0"
                max="1440"
                inputMode="numeric"
                onChange={(event) =>
                  updateScale("sleepMinutes", event.target.value)
                }
                placeholder="420"
                type="number"
                value={draft.sleepMinutes ?? ""}
              />
              <span>minutes</span>
            </div>
          </label>
          <ScaleSelect
            label="Mood"
            priorReference={prior.mood}
            value={draft.mood}
            onChange={(value) => updateScale("mood", value)}
          />
          <ScaleSelect
            label="Energy"
            priorReference={prior.energy}
            value={draft.energy}
            onChange={(value) => updateScale("energy", value)}
          />
        </div>

        <details className="optional-check-in-details">
          <summary>More optional fields and context</summary>
          <div className="field-grid">
            <ScaleSelect
              label="Sleep need"
              value={draft.perceivedSleepNeed}
              onChange={(value) => updateScale("perceivedSleepNeed", value)}
            />
            <ScaleSelect
              label="Irritability"
              priorReference={prior.irritability}
              value={draft.irritability}
              onChange={(value) => updateScale("irritability", value)}
            />
            <ScaleSelect
              label="Concentration"
              priorReference={prior.concentration}
              value={draft.concentration}
              onChange={(value) => updateScale("concentration", value)}
            />
            <ScaleSelect
              label="Impulsivity"
              priorReference={prior.impulsivity}
              value={draft.impulsivity}
              onChange={(value) => updateScale("impulsivity", value)}
            />
            <OptionalBoolean
              label="Medication taken"
              value={draft.medicationTaken}
              onChange={(value) =>
                updateOptionalBoolean("medicationTaken", value)
              }
            />
            <OptionalBoolean
              label="Substance use"
              value={draft.substanceUse}
              onChange={(value) => updateOptionalBoolean("substanceUse", value)}
            />
          </div>

          <fieldset className="context-fieldset">
            <legend>Context, if relevant</legend>
            <TagSelector
              selectedTags={draft.contextTags}
              onToggleTag={toggleTag}
            />
          </fieldset>
        </details>

        <div className="form-footer">
          <span className="form-hint">
            Stored in the encrypted local database.
          </span>
          <button
            className="primary-button"
            disabled={saving || !hasCheckInAnswers(draft)}
            type="submit"
          >
            {saving ? "Saving..." : "Save check-in"}
          </button>
        </div>
        {submittedLocalDate !== undefined && (
          <p className="check-in-reference-note">
            Saved as a separate check-in. Earlier-day medians are shown only as
            historical context.
          </p>
        )}
        {error !== undefined && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
      </form>
    </Panel>
  );
}

function ScaleSelect({
  label,
  value,
  priorReference,
  onChange,
}: {
  label: string;
  value?: number;
  priorReference?: PriorCheckInReference;
  onChange: (value: string) => void;
}) {
  return (
    <div className="field">
      <span>{label}</span>
      <div className="scale-buttons" role="group" aria-label={label}>
        <button
          className={`scale-skip${value === undefined ? " scale-active" : ""}`}
          onClick={() => onChange("")}
          type="button"
          aria-pressed={value === undefined}
        >
          –
        </button>
        {Array.from({ length: 7 }, (_, index) => index + 1).map((option) => (
          <button
            aria-pressed={value === option}
            className={`scale-pip${value === option ? " scale-active" : ""}`}
            key={option}
            onClick={() => onChange(String(option))}
            type="button"
          >
            {option}
          </button>
        ))}
      </div>
      {priorReference !== undefined && (
        <small className="scale-reference">
          Earlier logged days: median {formatCompact(priorReference.median)}/7
          across {priorReference.dayCount}{" "}
          {priorReference.dayCount === 1 ? "day" : "days"}.
        </small>
      )}
    </div>
  );
}

function OptionalBoolean({
  label,
  value,
  onChange,
}: {
  label: string;
  value?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <select
        onChange={(event) => onChange(event.target.value)}
        value={value === undefined ? "" : value ? "yes" : "no"}
      >
        <option value="">Skip</option>
        <option value="yes">Yes</option>
        <option value="no">No</option>
      </select>
    </label>
  );
}

function InsightsPanel({ model }: { model?: DailyRhythmDashboardModel }) {
  return (
    <Panel
      eyebrow="Personal baseline"
      title="Rhythm changes"
      description="Compares recent multi-day windows with your own earlier history. Descriptive only; you decide what it means."
    >
      {model === undefined && <PanelLoading text="Loading local history..." />}
      {model?.state === "unavailable" && (
        <EmptyState title="Insights unavailable" body={model.message} />
      )}
      {model?.state === "empty" && (
        <>
          <EmptyState title={model.emptyTitle} body={model.emptyBody}>
            <div className="baseline-progress">
              <span className="coverage">{model.coverageText}</span>
              {model.baselineProgressText !== undefined && (
                <span className="coverage">{model.baselineProgressText}</span>
              )}
            </div>
          </EmptyState>
          <ReadinessDetails items={model.readiness} />
        </>
      )}
      {model?.state === "ready" && (
        <>
          <div className="coverage-row">
            <span>{model.coverageText}</span>
            {model.isTruncated && <span>Showing recent results</span>}
          </div>
          <div className="insight-list">
            {model.cards.map((card) => (
              <article
                className="insight-card"
                key={`${card.localDate}-${card.dimensionLabel}`}
              >
                <div className="insight-heading">
                  <span>{card.dimensionLabel}</span>
                  <span>{card.changeLabel}</span>
                </div>
                <p>{card.summary}</p>
                <ul>
                  {card.evidence.map((evidence) => (
                    <li key={evidence}>{evidence}</li>
                  ))}
                </ul>
              </article>
            ))}
          </div>
          <ReadinessDetails items={model.readiness} />
        </>
      )}
    </Panel>
  );
}

function LatestLocalRecordPanel({
  model,
  onOpenHistory,
}: {
  model: ReturnType<typeof toLatestLocalRecordModel>;
  onOpenHistory: () => void;
}) {
  return (
    <Panel
      eyebrow="Private logbook"
      title="Latest local record"
      description="Your most recent recorded day."
    >
      {model.state === "unavailable" && (
        <EmptyState
          title="Latest record unavailable"
          body="Your stored records remain on this device."
        />
      )}
      {model.state === "empty" && (
        <EmptyState
          title="Nothing recorded yet"
          body="An optional check-in or local source will begin your log."
        />
      )}
      {model.state === "ready" && (
        <>
          <div className="coverage-row">
            <strong>{model.dateLabel}</strong>
            <span>{model.coverageLabel}</span>
          </div>
          <div className="latest-record-list">
            {model.entries.map((entry) => (
              <div className="latest-record-entry" key={entry.label}>
                <strong>{entry.label}</strong>
                <span>{entry.summary}</span>
              </div>
            ))}
          </div>
          <button
            className="secondary-button latest-record-link"
            onClick={onOpenHistory}
            type="button"
          >
            Open daily history →
          </button>
        </>
      )}
    </Panel>
  );
}

function ReadinessDetails({ items }: { items: ReadinessItemModel[] }) {
  if (items.length === 0) {
    return null;
  }

  return (
    <details className="readiness-details">
      <summary>Readiness by signal</summary>
      <div className="readiness-list">
        {items.map((item) => (
          <div
            className="readiness-row"
            data-state={item.state}
            key={item.dimensionLabel}
          >
            <div>
              <strong>{item.dimensionLabel}</strong>
              <span>{item.detail}</span>
            </div>
            <span>{item.statusLabel}</span>
          </div>
        ))}
      </div>
    </details>
  );
}

function CollectionPanel({
  controls,
  unavailable,
  onChange,
  onEnableStarterSet,
  onEnableOsActivitySet,
  onPrepareDelete,
}: {
  controls: SignalCollectionControlModel[];
  unavailable: boolean;
  onChange: (signal: SignalId, enabled: boolean) => Promise<void>;
  onEnableStarterSet: () => Promise<void>;
  onEnableOsActivitySet: () => Promise<void>;
  onPrepareDelete: (signal: SignalId) => Promise<void>;
}) {
  const [pending, setPending] = useState<SignalId>();
  const [reviewing, setReviewing] = useState<SignalId>();
  const [starterPending, setStarterPending] = useState(false);
  const [osStarterPending, setOsStarterPending] = useState(false);
  const browserControls = browserCollectionControls(controls);
  const osActivityControls = osActivityCollectionControls(controls);
  const osControls = osCollectionControls(controls);
  const starter = toBrowserStarterSetModel(browserControls);
  const osStarter = toOsActivityStarterSetModel(osActivityControls);

  async function toggle(control: SignalCollectionControlModel): Promise<void> {
    setPending(control.signal);
    await onChange(control.signal, !control.enabled);
    setPending(undefined);
  }

  async function reviewDeletion(
    control: SignalCollectionControlModel,
  ): Promise<void> {
    setReviewing(control.signal);
    await onPrepareDelete(control.signal);
    setReviewing(undefined);
  }

  async function enableStarter(): Promise<void> {
    setStarterPending(true);
    await onEnableStarterSet();
    setStarterPending(false);
  }

  async function enableOsStarter(): Promise<void> {
    setOsStarterPending(true);
    await onEnableOsActivitySet();
    setOsStarterPending(false);
  }

  return (
    <>
      <Panel
        id="os-activity-signals-panel"
        compact
        eyebrow="Collection"
        title="Computer activity signals"
        description="Optional active and idle time. No app names, window titles, or content."
      >
        {unavailable ? (
          <EmptyState
            title="Settings unavailable"
            body="Start the local service to manage computer activity settings."
          />
        ) : osControls.length === 0 ? (
          <PanelLoading text="Loading computer activity settings..." />
        ) : (
          <>
            <div className="starter-set-card">
              <div>
                <strong>Computer activity starter set</strong>
                <span>{osStarter.labels.join(", ")}.</span>
                <small>
                  {osStarter.statusText}. These are local duration totals only.
                </small>
              </div>
              <button
                className="secondary-button"
                disabled={osStarterPending || osStarter.fullyEnabled}
                onClick={() => void enableOsStarter()}
                type="button"
              >
                {osStarterPending
                  ? "Enabling..."
                  : osStarter.fullyEnabled
                    ? "Computer activity enabled"
                    : "Enable computer activity"}
              </button>
            </div>
            <SignalControlList
              controls={osControls}
              pending={pending}
              reviewing={reviewing}
              onReviewDeletion={reviewDeletion}
              onToggle={toggle}
            />
          </>
        )}
      </Panel>

      <Panel
        id="browser-signals-panel"
        compact
        eyebrow="Collection"
        title="Browser signals"
        description="Each signal is controlled individually. No URLs, titles, or page text."
      >
        {unavailable ? (
          <EmptyState
            title="Settings unavailable"
            body="Start the local service to manage browser settings."
          />
        ) : browserControls.length === 0 ? (
          <PanelLoading text="Loading signal settings..." />
        ) : (
          <>
            <div className="starter-set-card">
              <div>
                <strong>Browser starter set</strong>
                <span>{starter.labels.join(", ")}.</span>
                <small>
                  {starter.statusText}. Browser aggregates appear after a
                  15-minute period closes. Active and idle time may require
                  Chrome's optional idle permission.
                </small>
              </div>
              <button
                className="secondary-button"
                disabled={starterPending || starter.fullyEnabled}
                onClick={() => void enableStarter()}
                type="button"
              >
                {starterPending
                  ? "Enabling..."
                  : starter.fullyEnabled
                    ? "Starter set enabled"
                    : "Enable starter set"}
              </button>
            </div>
            <SignalControlList
              controls={browserControls}
              pending={pending}
              reviewing={reviewing}
              onReviewDeletion={reviewDeletion}
              onToggle={toggle}
            />
          </>
        )}
      </Panel>
    </>
  );
}

function SignalControlList({
  controls,
  pending,
  reviewing,
  onReviewDeletion,
  onToggle,
}: {
  controls: SignalCollectionControlModel[];
  pending?: SignalId;
  reviewing?: SignalId;
  onReviewDeletion: (control: SignalCollectionControlModel) => Promise<void>;
  onToggle: (control: SignalCollectionControlModel) => Promise<void>;
}) {
  return (
    <div className="control-list">
      {controls.map((control) => (
        <div className="control-row" key={control.signal}>
          <div className="control-copy">
            <strong>{control.label}</strong>
            <span>{control.description}</span>
          </div>
          <div className="control-actions">
            <button
              className="quiet-button signal-delete-button"
              disabled={reviewing === control.signal}
              onClick={() => void onReviewDeletion(control)}
              type="button"
            >
              {reviewing === control.signal ? "Reviewing..." : "Delete history"}
            </button>
            <button
              role="switch"
              aria-checked={control.enabled}
              aria-label={control.label}
              className="switch"
              data-enabled={control.enabled}
              disabled={pending === control.signal}
              onClick={() => void onToggle(control)}
              title={control.statusText}
              type="button"
            >
              <span />
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

function DeleteSignalRecordsDialog({
  model,
  onCancel,
  onConfirm,
}: {
  model: SignalDeletionConfirmationModel;
  onCancel: () => void;
  onConfirm: () => Promise<void>;
}) {
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    function handleEscape(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        onCancel();
      }
    }
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onCancel]);

  async function remove(): Promise<void> {
    setDeleting(true);
    await onConfirm();
    setDeleting(false);
  }

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        aria-labelledby="delete-signal-dialog-title"
        aria-modal="true"
        className="dialog"
        role="dialog"
      >
        <p className="eyebrow">Delete signal history</p>
        <h2 id="delete-signal-dialog-title">Delete {model.label} history?</h2>
        <p className="dialog-summary">{model.summaryText}</p>
        <p>{model.confirmationText}</p>
        <p className="small-copy">
          Confirmation expires at {model.expiresAt}. Consent history and source
          replay protection remain stored.
        </p>
        <div className="dialog-actions">
          <button
            className="secondary-button"
            disabled={deleting}
            onClick={onCancel}
            type="button"
          >
            Cancel
          </button>
          <button
            className="primary-button destructive"
            disabled={deleting || model.isEmpty}
            onClick={() => void remove()}
            type="button"
          >
            {deleting ? "Deleting..." : "Delete this signal's history"}
          </button>
        </div>
      </section>
    </div>
  );
}

function DataPanel({
  model,
  unavailable,
  onPrepareClear,
  onPrepareExport,
  onPrepareBackup,
  onOpenRestore,
  onPrepareRemoval,
}: {
  model?: LocalDataControlModel;
  unavailable: boolean;
  onPrepareClear: () => Promise<void>;
  onPrepareExport: () => Promise<void>;
  onPrepareBackup: () => Promise<void>;
  onOpenRestore: () => void;
  onPrepareRemoval: () => void;
}) {
  return (
    <Panel
      compact
      eyebrow="Local data"
      title={model?.title ?? "Stored records"}
      description={model?.summaryText ?? "Reading your local record counts..."}
    >
      {unavailable ? (
        <EmptyState
          title="Record counts unavailable"
          body="The encrypted database remains owned by the local service."
        />
      ) : (
        <>
          <p className="small-copy">
            {model?.confirmationText ??
              "Records are managed through the local service."}
          </p>
          <div className="button-row">
            <button
              className="primary-button"
              disabled={model === undefined || model.isEmpty}
              onClick={() => void onPrepareBackup()}
              type="button"
            >
              Create encrypted backup
            </button>
            <button
              className="secondary-button"
              disabled={model === undefined || model.isEmpty}
              onClick={() => void onPrepareExport()}
              type="button"
            >
              Review export
            </button>
            <button
              className="secondary-button danger-button"
              disabled={model === undefined || model.isEmpty}
              onClick={() => void onPrepareClear()}
              type="button"
            >
              Review clearing
            </button>
          </div>
          <p className="small-copy">
            Backups use a separate recovery secret that mindcanary does not
            store. CSV export is a separate action.
          </p>
        </>
      )}
      <div className="button-row">
        <button
          className="secondary-button"
          onClick={onOpenRestore}
          type="button"
        >
          Verify or restore backup
        </button>
        <button
          className="secondary-button danger-button"
          onClick={onPrepareRemoval}
          type="button"
        >
          Review app removal
        </button>
      </div>
    </Panel>
  );
}

function SupportPanel({
  appVersion,
  onPreview,
}: {
  appVersion?: string;
  onPreview: () => void;
}) {
  return (
    <Panel
      compact
      eyebrow="Support"
      title={
        appVersion === undefined ? "mindcanary" : `mindcanary ${appVersion}`
      }
      description="Preview non-sensitive system status for a support request."
    >
      <p className="small-copy">
        Nothing leaves this device unless you copy and send it.
      </p>
      <div className="button-row">
        <button
          className="secondary-button"
          disabled={appVersion === undefined}
          onClick={onPreview}
          type="button"
        >
          Preview support information
        </button>
      </div>
    </Panel>
  );
}

function SupportDiagnosticsDialog({
  model,
  onClose,
  onCopied,
}: {
  model: SupportDiagnosticsModel;
  onClose: () => void;
  onCopied: () => void;
}) {
  const [copyFailed, setCopyFailed] = useState(false);

  async function copyReport(): Promise<void> {
    try {
      await navigator.clipboard.writeText(model.reportText);
      setCopyFailed(false);
      onCopied();
    } catch {
      setCopyFailed(true);
    }
  }

  return (
    <Dialog
      eyebrow="Support information"
      onClose={onClose}
      title={`mindcanary ${model.appVersion}`}
      wide
    >
      <p>Review this text before sharing it. Nothing is sent automatically.</p>
      <textarea
        aria-label="Support information preview"
        className="support-diagnostics-preview"
        readOnly
        value={model.reportText}
      />
      <div className="dialog-actions">
        {copyFailed && (
          <span className="form-hint">
            Copy failed. Select the text manually.
          </span>
        )}
        <button className="quiet-button" onClick={onClose} type="button">
          Close
        </button>
        <button
          className="primary-button"
          onClick={() => void copyReport()}
          type="button"
        >
          Copy
        </button>
      </div>
    </Dialog>
  );
}

function LocalRemovalDialog({
  model,
  onCancel,
  onConfirm,
}: {
  model: LocalRemovalModel;
  onCancel: () => void;
  onConfirm: (confirmationPhrase: string) => Promise<void>;
}) {
  const [removing, setRemoving] = useState(false);
  const [confirmationPhrase, setConfirmationPhrase] = useState("");
  const canConfirm = confirmationPhrase === model.confirmationPhrase;

  useEffect(() => {
    function handleEscape(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        onCancel();
      }
    }
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onCancel]);

  async function remove(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!canConfirm) {
      return;
    }
    setRemoving(true);
    await onConfirm(confirmationPhrase);
    setRemoving(false);
  }

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        aria-labelledby="local-removal-dialog-title"
        aria-modal="true"
        className="dialog"
        role="dialog"
      >
        <p className="eyebrow">Remove local installation</p>
        <h2 id="local-removal-dialog-title">{model.title}</h2>
        <p className="dialog-summary">{model.summaryText}</p>
        <p>{model.confirmationText}</p>
        <p className="small-copy">{model.excludedText}</p>
        <form onSubmit={(event) => void remove(event)}>
          <label className="field-label" htmlFor="local-removal-phrase">
            Type {model.confirmationPhrase}
          </label>
          <input
            autoFocus
            disabled={removing}
            id="local-removal-phrase"
            onChange={(event) =>
              setConfirmationPhrase(event.currentTarget.value)
            }
            type="text"
            value={confirmationPhrase}
          />
          <div className="dialog-actions">
            <button
              className="secondary-button"
              disabled={removing}
              onClick={onCancel}
              type="button"
            >
              Cancel
            </button>
            <button
              className="primary-button destructive"
              disabled={removing || !canConfirm}
              type="submit"
            >
              {removing ? "Removing..." : "Remove app-owned local data"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}

function ExportRecordsDialog({
  model,
  onCancel,
  onConfirm,
}: {
  model: ExportLocalRecordsConfirmationModel;
  onCancel: () => void;
  onConfirm: (exportDirectory: string) => Promise<void>;
}) {
  const [exporting, setExporting] = useState(false);
  const [exportDirectory, setExportDirectory] = useState("");

  useEffect(() => {
    function handleEscape(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        onCancel();
      }
    }
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onCancel]);

  async function exportRecords(
    event: FormEvent<HTMLFormElement>,
  ): Promise<void> {
    event.preventDefault();
    if (exportDirectory.trim().length === 0) {
      return;
    }
    setExporting(true);
    await onConfirm(exportDirectory.trim());
    setExporting(false);
  }

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        aria-labelledby="export-dialog-title"
        aria-modal="true"
        className="dialog"
        role="dialog"
      >
        <p className="eyebrow">Confirm local export</p>
        <h2 id="export-dialog-title">Export these local records?</h2>
        <p className="dialog-summary">{model.summaryText}</p>
        <p>{model.confirmationText}</p>
        <form onSubmit={(event) => void exportRecords(event)}>
          <label className="field-label" htmlFor="export-directory">
            Export folder
          </label>
          <input
            autoFocus
            disabled={exporting}
            id="export-directory"
            onChange={(event) => setExportDirectory(event.currentTarget.value)}
            placeholder="/home/you/Documents/mindcanary-export"
            type="text"
            value={exportDirectory}
          />
          <p className="small-copy">
            Use an absolute local folder path. Confirmation expires at{" "}
            {model.expiresAt}.
          </p>
          <div className="dialog-actions">
            <button
              className="secondary-button"
              disabled={exporting}
              onClick={onCancel}
              type="button"
            >
              Cancel
            </button>
            <button
              className="primary-button"
              disabled={
                exporting || model.isEmpty || exportDirectory.trim() === ""
              }
              type="submit"
            >
              {exporting ? "Exporting..." : "Write local export"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}

function CreateBackupDialog({
  model,
  onCancel,
  onConfirm,
}: {
  model: BackupConfirmationModel;
  onCancel: () => void;
  onConfirm: (backupPath: string) => Promise<void>;
}) {
  const [backupPath, setBackupPath] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    function handleEscape(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        onCancel();
      }
    }
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onCancel]);

  async function create(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (backupPath.trim().length === 0 || model.isEmpty) {
      return;
    }
    setCreating(true);
    await onConfirm(backupPath.trim());
    setCreating(false);
  }

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        aria-labelledby="backup-dialog-title"
        aria-modal="true"
        className="dialog"
        role="dialog"
      >
        <p className="eyebrow">Encrypted local backup</p>
        <h2 id="backup-dialog-title">Create a portable backup?</h2>
        <p className="dialog-summary">{model.summaryText}</p>
        <p>
          The daemon writes one SQLCipher-encrypted file and generates a
          separate 256-bit recovery secret. No plaintext staging file is used.
        </p>
        <form onSubmit={(event) => void create(event)}>
          <label className="field-label" htmlFor="backup-path">
            New backup file
          </label>
          <input
            autoFocus
            disabled={creating}
            id="backup-path"
            onChange={(event) => setBackupPath(event.currentTarget.value)}
            placeholder="/home/you/Documents/mindcanary-2026-06-19.mcbak"
            type="text"
            value={backupPath}
          />
          <p className="small-copy">
            Use a new absolute file path in an existing folder. Confirmation
            expires at {model.expiresAt}.
          </p>
          <div className="dialog-actions">
            <button
              className="secondary-button"
              disabled={creating}
              onClick={onCancel}
              type="button"
            >
              Cancel
            </button>
            <button
              className="primary-button"
              disabled={creating || model.isEmpty || backupPath.trim() === ""}
              type="submit"
            >
              {creating ? "Encrypting..." : "Create encrypted backup"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}

function BackupCreatedDialog({
  model,
  onClose,
}: {
  model: CreatedBackupModel;
  onClose: () => void;
}) {
  useEffect(() => {
    function handleEscape(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        onClose();
      }
    }
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onClose]);

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        aria-labelledby="backup-created-title"
        aria-modal="true"
        className="dialog backup-result-dialog"
        role="dialog"
      >
        <p className="eyebrow">Backup created</p>
        <h2 id="backup-created-title">Store the recovery secret separately</h2>
        <p className="dialog-summary">{model.summaryText}</p>
        <p>
          This secret is shown once and not stored anywhere. The backup file is
          unusable without it.
        </p>
        <label className="field-label" htmlFor="backup-created-path">
          Backup file
        </label>
        <input
          id="backup-created-path"
          onFocus={(event) => event.currentTarget.select()}
          readOnly
          value={model.backupPath}
        />
        <label className="field-label" htmlFor="backup-recovery-secret">
          Recovery secret, shown once
        </label>
        <textarea
          id="backup-recovery-secret"
          onFocus={(event) => event.currentTarget.select()}
          readOnly
          rows={3}
          value={model.recoverySecret}
        />
        <p className="small-copy">
          Created {formatDateTime(model.createdAt)} · backup format v
          {model.formatVersion}
        </p>
        <div className="dialog-actions">
          <button className="primary-button" onClick={onClose} type="button">
            I stored the secret
          </button>
        </div>
      </section>
    </div>
  );
}

function RestoreBackupDialog({
  profileEmpty,
  onCancel,
  onVerify,
  onRestore,
}: {
  profileEmpty: boolean;
  onCancel: () => void;
  onVerify: (
    backupPath: string,
    recoverySecret: string,
  ) => Promise<VerifiedBackupModel>;
  onRestore: (
    backupPath: string,
    recoverySecret: string,
  ) => Promise<RestoredBackupModel>;
}) {
  const [backupPath, setBackupPath] = useState("");
  const [recoverySecret, setRecoverySecret] = useState("");
  const [verified, setVerified] = useState<VerifiedBackupModel>();
  const [working, setWorking] = useState<"verify" | "restore">();
  const [error, setError] = useState<string>();
  const complete =
    backupPath.trim().length > 0 && recoverySecret.trim().length > 0;

  useEffect(() => {
    function handleEscape(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        onCancel();
      }
    }
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onCancel]);

  function changePath(value: string): void {
    setBackupPath(value);
    setVerified(undefined);
    setError(undefined);
  }

  function changeSecret(value: string): void {
    setRecoverySecret(value);
    setVerified(undefined);
    setError(undefined);
  }

  async function verify(): Promise<void> {
    if (!complete) {
      return;
    }
    setWorking("verify");
    setError(undefined);
    try {
      setVerified(await onVerify(backupPath.trim(), recoverySecret.trim()));
    } catch {
      setVerified(undefined);
      setError("The file and recovery secret could not be verified.");
    } finally {
      setWorking(undefined);
    }
  }

  async function restore(): Promise<void> {
    if (verified === undefined || !profileEmpty) {
      return;
    }
    setWorking("restore");
    setError(undefined);
    try {
      await onRestore(backupPath.trim(), recoverySecret.trim());
    } catch {
      setError("Restore failed without replacing the current local records.");
      setWorking(undefined);
    }
  }

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        aria-labelledby="restore-backup-title"
        aria-modal="true"
        className="dialog backup-result-dialog"
        role="dialog"
      >
        <p className="eyebrow">Portable recovery</p>
        <h2 id="restore-backup-title">Verify or restore a backup</h2>
        <p>
          Verification reads the encrypted format and integrity metadata. It
          does not change this profile.
        </p>
        <label className="field-label" htmlFor="restore-backup-path">
          Backup file
        </label>
        <input
          autoFocus
          disabled={working !== undefined}
          id="restore-backup-path"
          onChange={(event) => changePath(event.currentTarget.value)}
          placeholder="/home/you/Documents/mindcanary-backup.mcbak"
          type="text"
          value={backupPath}
        />
        <label className="field-label" htmlFor="restore-recovery-secret">
          Recovery secret
        </label>
        <textarea
          disabled={working !== undefined}
          id="restore-recovery-secret"
          onChange={(event) => changeSecret(event.currentTarget.value)}
          rows={3}
          value={recoverySecret}
        />
        {verified !== undefined && (
          <p className="dialog-summary">
            Verified backup from {formatDateTime(verified.createdAt)} · format v
            {verified.formatVersion} · schema v{verified.schemaVersion}
          </p>
        )}
        {!profileEmpty && (
          <p className="small-copy">
            Restore is available only when local records are empty, so an
            existing history can never be silently replaced. Create a backup
            before clearing anything you want to keep.
          </p>
        )}
        {error !== undefined && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        <div className="dialog-actions">
          <button
            className="secondary-button"
            disabled={working !== undefined}
            onClick={onCancel}
            type="button"
          >
            Cancel
          </button>
          <button
            className="secondary-button"
            disabled={!complete || working !== undefined}
            onClick={() => void verify()}
            type="button"
          >
            {working === "verify" ? "Verifying..." : "Verify backup"}
          </button>
          <button
            className="primary-button"
            disabled={
              verified === undefined || !profileEmpty || working !== undefined
            }
            onClick={() => void restore()}
            type="button"
          >
            {working === "restore" ? "Restoring..." : "Restore into profile"}
          </button>
        </div>
      </section>
    </div>
  );
}

function ClearRecordsDialog({
  model,
  onCancel,
  onConfirm,
}: {
  model: ClearLocalRecordsConfirmationModel;
  onCancel: () => void;
  onConfirm: () => Promise<void>;
}) {
  const [clearing, setClearing] = useState(false);

  useEffect(() => {
    function handleEscape(event: KeyboardEvent): void {
      if (event.key === "Escape") {
        onCancel();
      }
    }
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onCancel]);

  async function clear(): Promise<void> {
    setClearing(true);
    await onConfirm();
    setClearing(false);
  }

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        aria-labelledby="clear-dialog-title"
        aria-modal="true"
        className="dialog"
        role="dialog"
      >
        <p className="eyebrow">Confirm local clearing</p>
        <h2 id="clear-dialog-title">Clear these local records?</h2>
        <p className="dialog-summary">{model.summaryText}</p>
        <p>{model.confirmationText}</p>
        <p className="small-copy">
          Confirmation expires at {model.expiresAt}. The database file and key
          are not removed.
        </p>
        <div className="dialog-actions">
          <button
            className="secondary-button"
            disabled={clearing}
            onClick={onCancel}
            type="button"
          >
            Cancel
          </button>
          <button
            className="primary-button destructive"
            disabled={clearing}
            onClick={() => void clear()}
            type="button"
          >
            {clearing ? "Clearing..." : "Clear local records"}
          </button>
        </div>
      </section>
    </div>
  );
}

function Panel({
  id,
  eyebrow,
  title,
  description,
  compact = false,
  children,
}: {
  id?: string;
  eyebrow: string;
  title: string;
  description: string;
  compact?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="panel" data-compact={compact} id={id}>
      <div className="panel-heading">
        <p className="eyebrow">{eyebrow}</p>
        <h2>{title}</h2>
        <p>{description}</p>
      </div>
      {children}
    </section>
  );
}

function PanelLoading({ text }: { text: string }) {
  return (
    <div className="panel-loading">
      <span className="loading-dot" aria-hidden="true" />
      {text}
    </div>
  );
}

function EmptyState({
  title,
  body,
  children,
}: {
  title: string;
  body: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <strong>{title}</strong>
      <p>{body}</p>
      {children}
    </div>
  );
}

function AnnotationDeleteDialog({
  model,
  onCancel,
  onConfirm,
}: {
  model: AnnotationDeletionConfirmationModel;
  onCancel: () => void;
  onConfirm: () => Promise<void>;
}) {
  const [deleting, setDeleting] = useState(false);

  async function remove(): Promise<void> {
    setDeleting(true);
    await onConfirm();
    setDeleting(false);
  }

  return (
    <Dialog
      eyebrow="Delete note"
      onClose={onCancel}
      title="Delete this note?"
      tone="danger"
    >
      <p className="dialog-summary">
        This note will be removed from your local history. This cannot be
        undone.
      </p>
      <p className="small-copy">Confirmation expires at {model.expiresAt}.</p>
      <div className="dialog-actions">
        <button
          className="secondary-button"
          disabled={deleting}
          onClick={onCancel}
          type="button"
        >
          Cancel
        </button>
        <button
          className="primary-button destructive"
          disabled={deleting}
          onClick={() => void remove()}
          type="button"
        >
          {deleting ? "Deleting..." : "Delete note"}
        </button>
      </div>
    </Dialog>
  );
}

function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}
