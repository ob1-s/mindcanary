import { invoke } from "@tauri-apps/api/core";

import type {
  AnnotationRecord,
  CheckInRecord,
  ProtocolResponse,
  SignalId,
} from "@mindcanary/protocol";

import type { LocalRemovalReport } from "./local-removal";

export type ChromeConnectorRuntime = "development" | "packaged";

export type ChromeConnectorHealth =
  | "missing"
  | "ready"
  | "needs_repair"
  | "helper_missing"
  | "unavailable";

export interface ChromeConnectorStatus {
  runtime: ChromeConnectorRuntime;
  health: ChromeConnectorHealth;
  setup_command: string | null;
}

export interface RuntimeDiagnostics {
  runtime: "development" | "packaged";
  profile: string | null;
  socket_path: string;
}

export interface LocalServiceAutostartStatus {
  supported: boolean;
  enabled: boolean;
  active: boolean;
}

export const daemonApi = {
  appVersion: () => invoke<string>("app_version"),
  runtimeDiagnostics: () => invoke<RuntimeDiagnostics>("runtime_diagnostics"),
  ensureLocalService: () => invoke<void>("ensure_local_service"),
  localServiceAutostartStatus: () =>
    invoke<LocalServiceAutostartStatus>("local_service_autostart_status"),
  setLocalServiceAutostart: (enabled: boolean) =>
    invoke<LocalServiceAutostartStatus>("set_local_service_autostart", {
      enabled,
    }),
  chromeConnectorStatus: () =>
    invoke<ChromeConnectorStatus>("chrome_connector_status"),
  connectChrome: () => invoke<ChromeConnectorStatus>("connect_chrome"),
  completeLocalRemoval: (confirmationPhrase: string) =>
    invoke<LocalRemovalReport>("complete_local_removal", {
      confirmationPhrase,
    }),
  health: () => invoke<ProtocolResponse>("daemon_health"),
  sourceStatus: () => invoke<ProtocolResponse>("source_status"),
  insights: (limit = 20) =>
    invoke<ProtocolResponse>("daily_rhythm_insights", { limit }),
  timeline: (limit = 30) =>
    invoke<ProtocolResponse>("daily_timeline", { limit }),
  collectionSettings: () => invoke<ProtocolResponse>("collection_settings"),
  platformCapabilities: () => invoke<ProtocolResponse>("platform_capabilities"),
  setSignalCollection: (signal: SignalId, enabled: boolean) =>
    invoke<ProtocolResponse>("set_signal_collection", { signal, enabled }),
  prepareDeleteSignalRecords: (signal: SignalId) =>
    invoke<ProtocolResponse>("prepare_delete_signal_records", { signal }),
  deleteSignalRecords: (signal: SignalId, confirmationToken: string) =>
    invoke<ProtocolResponse>("delete_signal_records", {
      signal,
      confirmationToken,
    }),
  submitCheckIn: (checkIn: CheckInRecord) =>
    invoke<ProtocolResponse>("submit_check_in", { checkIn }),
  prepareDeleteLatestCheckIn: (localDate: string) =>
    invoke<ProtocolResponse>("prepare_delete_latest_check_in", { localDate }),
  deleteLatestCheckIn: (localDate: string, confirmationToken: string) =>
    invoke<ProtocolResponse>("delete_latest_check_in", {
      localDate,
      confirmationToken,
    }),
  saveAnnotation: (annotation: AnnotationRecord) =>
    invoke<ProtocolResponse>("save_annotation", { annotation }),
  prepareDeleteAnnotation: (annotationId: string) =>
    invoke<ProtocolResponse>("prepare_delete_annotation", { annotationId }),
  deleteAnnotation: (annotationId: string, confirmationToken: string) =>
    invoke<ProtocolResponse>("delete_annotation", {
      annotationId,
      confirmationToken,
    }),
  localDataSummary: () => invoke<ProtocolResponse>("local_data_summary"),
  prepareExportLocalRecords: () =>
    invoke<ProtocolResponse>("prepare_export_local_records"),
  exportLocalRecords: (confirmationToken: string, exportDirectory: string) =>
    invoke<ProtocolResponse>("export_local_records", {
      confirmationToken,
      exportDirectory,
    }),
  prepareCreateLocalBackup: () =>
    invoke<ProtocolResponse>("prepare_create_local_backup"),
  createLocalBackup: (confirmationToken: string, backupPath: string) =>
    invoke<ProtocolResponse>("create_local_backup", {
      confirmationToken,
      backupPath,
    }),
  verifyLocalBackup: (backupPath: string, recoverySecret: string) =>
    invoke<ProtocolResponse>("verify_local_backup", {
      backupPath,
      recoverySecret,
    }),
  restoreLocalBackup: (backupPath: string, recoverySecret: string) =>
    invoke<ProtocolResponse>("restore_local_backup", {
      backupPath,
      recoverySecret,
    }),
  prepareClearLocalRecords: () =>
    invoke<ProtocolResponse>("prepare_clear_local_records"),
  clearLocalRecords: (confirmationToken: string) =>
    invoke<ProtocolResponse>("clear_local_records", { confirmationToken }),
};
