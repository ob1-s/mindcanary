import {
  PROTOCOL_VERSION,
  type LocalDataExport,
  type LocalDataSummary,
  type ProtocolRequest,
  type ProtocolResponse,
} from "@mindcanary/protocol";

import type { PlatformCapabilityModel } from "./platform";
import type { LocalServiceState } from "./setup";
import type { ConnectionStatusModel } from "./source-status";

export interface SupportDiagnosticsModel {
  appVersion: string;
  reportText: string;
}

export interface SupportDiagnosticsInput {
  appVersion: string;
  serviceState: LocalServiceState;
  connections?: ConnectionStatusModel;
  platform?: PlatformCapabilityModel;
  localDataAvailable: boolean;
}

export interface LocalDataControlModel {
  title: string;
  summaryText: string;
  confirmationText: string;
  isEmpty: boolean;
}

export interface ClearLocalRecordsConfirmationModel extends LocalDataControlModel {
  confirmationToken: string;
  expiresAt: string;
}

export interface ExportLocalRecordsConfirmationModel extends LocalDataControlModel {
  confirmationToken: string;
  expiresAt: string;
}

export interface LocalDataExportModel extends LocalDataControlModel {
  exportDirectory: string;
  reportPath: string;
  dailyBrowserCsvPath: string;
  dailyOsCsvPath: string;
  dailyCheckInCsvPath: string;
  annotationsCsvPath: string;
}

export function createSupportDiagnostics(
  input: SupportDiagnosticsInput,
): SupportDiagnosticsModel {
  const lines = [
    "mindcanary support information",
    "Report format: 1",
    `App version: ${input.appVersion}`,
    `Protocol version: ${PROTOCOL_VERSION}`,
    `Local service: ${input.serviceState}`,
    `Local data summary: ${input.localDataAvailable ? "available" : "unavailable"}`,
    `Environment: ${
      input.platform?.state === "ready"
        ? input.platform.environmentText
        : "unavailable"
    }`,
    "Source status:",
    ...diagnosticSourceLines(input.connections),
    "",
    "Privacy: this preview contains no check-in values, annotations, activity measurements, URLs, filenames, or database paths.",
  ];

  return {
    appVersion: input.appVersion,
    reportText: lines.join("\n"),
  };
}

function diagnosticSourceLines(
  connections: ConnectionStatusModel | undefined,
): string[] {
  if (connections === undefined) {
    return ["- unavailable"];
  }
  if (connections.state !== "ready") {
    return ["- unavailable"];
  }
  return connections.items.map(
    (item) => `- ${item.label}: ${item.statusLabel}`,
  );
}

export function createLocalDataSummaryRequest(): ProtocolRequest {
  return {
    type: "get_local_data_summary",
    protocol_version: PROTOCOL_VERSION,
  };
}

export function createPrepareExportLocalRecordsRequest(): ProtocolRequest {
  return {
    type: "prepare_export_local_records",
    protocol_version: PROTOCOL_VERSION,
  };
}

export function createExportLocalRecordsRequest(
  confirmationToken: string,
  exportDirectory: string,
): ProtocolRequest {
  if (confirmationToken.length === 0) {
    throw new TypeError("A confirmation token is required.");
  }
  if (exportDirectory.trim().length === 0) {
    throw new TypeError("An export directory is required.");
  }

  return {
    type: "export_local_records",
    protocol_version: PROTOCOL_VERSION,
    confirmation_token: confirmationToken,
    export_directory: exportDirectory,
  };
}

export function createPrepareClearLocalRecordsRequest(): ProtocolRequest {
  return {
    type: "prepare_clear_local_records",
    protocol_version: PROTOCOL_VERSION,
  };
}

export function createClearLocalRecordsRequest(
  confirmationToken: string,
): ProtocolRequest {
  if (confirmationToken.length === 0) {
    throw new TypeError("A confirmation token is required.");
  }

  return {
    type: "clear_local_records",
    protocol_version: PROTOCOL_VERSION,
    confirmation_token: confirmationToken,
  };
}

export function toLocalDataControlModel(
  response: ProtocolResponse,
):
  | LocalDataControlModel
  | ClearLocalRecordsConfirmationModel
  | ExportLocalRecordsConfirmationModel
  | LocalDataExportModel {
  switch (response.type) {
    case "local_data_summary":
      return summaryModel(response.summary);
    case "export_local_records_confirmation":
      return {
        ...exportSummaryModel(response.summary),
        confirmationToken: response.confirmation_token,
        expiresAt: response.expires_at,
      };
    case "local_records_exported":
      return exportResultModel(response.export);
    case "clear_local_records_confirmation":
      return {
        ...summaryModel(response.summary),
        confirmationToken: response.confirmation_token,
        expiresAt: response.expires_at,
      };
    case "local_records_cleared":
      return {
        title: "Local records cleared",
        summaryText: formatSummary(response.deleted),
        confirmationText:
          "The database file and key are still on this device, but app-owned records have been cleared.",
        isEmpty: true,
      };
    default:
      throw new TypeError("Unexpected response for local data controls.");
  }
}

function exportSummaryModel(summary: LocalDataSummary): LocalDataControlModel {
  const isEmpty = isSummaryEmpty(summary);
  return {
    title: isEmpty ? "Nothing to export" : "Export local records",
    summaryText: formatSummary(summary),
    confirmationText: isEmpty
      ? "There are no records to export."
      : "This writes a summary, CSV files, and your private notes. No URLs, titles, page text, or clinical labels are included.",
    isEmpty,
  };
}

function exportResultModel(exported: LocalDataExport): LocalDataExportModel {
  return {
    title: "Local export written",
    summaryText: formatSummary(exported.summary),
    confirmationText:
      "The export files were written on this device. Moving or syncing that folder is your choice.",
    isEmpty: isSummaryEmpty(exported.summary),
    exportDirectory: exported.export_directory,
    reportPath: exported.report_path,
    dailyBrowserCsvPath: exported.daily_browser_csv_path,
    dailyOsCsvPath: exported.daily_os_csv_path,
    dailyCheckInCsvPath: exported.daily_check_in_csv_path,
    annotationsCsvPath: exported.annotations_csv_path,
  };
}

function summaryModel(summary: LocalDataSummary): LocalDataControlModel {
  const isEmpty = isSummaryEmpty(summary);
  return {
    title: isEmpty ? "No local records" : "Clear local records",
    summaryText: formatSummary(summary),
    confirmationText: isEmpty
      ? "There are no records to clear."
      : "This clears app-owned records from this device. It does not uninstall mindcanary, remove the database key, or delete exports, backups, or browser extension storage.",
    isEmpty,
  };
}

function formatSummary(summary: LocalDataSummary): string {
  return [
    pluralize(
      summary.aggregate_batch_count,
      "aggregate batch",
      "aggregate batches",
    ),
    pluralize(
      summary.aggregate_metric_count,
      "aggregate metric",
      "aggregate metrics",
    ),
    pluralize(summary.check_in_count, "check-in", "check-ins"),
    pluralize(summary.context_tag_count, "context tag", "context tags"),
    pluralize(summary.annotation_count, "annotation", "annotations"),
    pluralize(
      summary.annotation_context_tag_count,
      "annotation tag",
      "annotation tags",
    ),
  ].join(", ");
}

function isSummaryEmpty(summary: LocalDataSummary): boolean {
  return (
    summary.aggregate_batch_count === 0 &&
    summary.aggregate_metric_count === 0 &&
    summary.check_in_count === 0 &&
    summary.context_tag_count === 0 &&
    summary.annotation_count === 0 &&
    summary.annotation_context_tag_count === 0
  );
}

function pluralize(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}
