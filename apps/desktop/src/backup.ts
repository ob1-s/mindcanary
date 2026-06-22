import {
  PROTOCOL_VERSION,
  type LocalBackup,
  type LocalBackupMetadata,
  type LocalDataSummary,
  type ProtocolRequest,
  type ProtocolResponse,
} from "@mindcanary/protocol";

export interface BackupConfirmationModel {
  confirmationToken: string;
  expiresAt: string;
  summaryText: string;
  isEmpty: boolean;
}

export interface CreatedBackupModel {
  backupPath: string;
  createdAt: string;
  formatVersion: number;
  recoverySecret: string;
  summaryText: string;
}

export interface VerifiedBackupModel {
  backupPath: string;
  createdAt: string;
  formatVersion: number;
  schemaVersion: number;
}

export interface RestoredBackupModel {
  backup: VerifiedBackupModel;
  summaryText: string;
}

export function createPrepareLocalBackupRequest(): ProtocolRequest {
  return {
    type: "prepare_create_local_backup",
    protocol_version: PROTOCOL_VERSION,
  };
}

export function createLocalBackupRequest(
  confirmationToken: string,
  backupPath: string,
): ProtocolRequest {
  requireValue(confirmationToken, "confirmation token");
  requireValue(backupPath, "backup path");
  return {
    type: "create_local_backup",
    protocol_version: PROTOCOL_VERSION,
    confirmation_token: confirmationToken,
    backup_path: backupPath.trim(),
  };
}

export function createVerifyLocalBackupRequest(
  backupPath: string,
  recoverySecret: string,
): ProtocolRequest {
  requireValue(backupPath, "backup path");
  requireValue(recoverySecret, "recovery secret");
  return {
    type: "verify_local_backup",
    protocol_version: PROTOCOL_VERSION,
    backup_path: backupPath.trim(),
    recovery_secret: recoverySecret.trim(),
  };
}

export function createRestoreLocalBackupRequest(
  backupPath: string,
  recoverySecret: string,
): ProtocolRequest {
  requireValue(backupPath, "backup path");
  requireValue(recoverySecret, "recovery secret");
  return {
    type: "restore_local_backup",
    protocol_version: PROTOCOL_VERSION,
    backup_path: backupPath.trim(),
    recovery_secret: recoverySecret.trim(),
  };
}

export function toBackupConfirmationModel(
  response: ProtocolResponse,
): BackupConfirmationModel {
  if (response.type !== "create_local_backup_confirmation") {
    throw new TypeError("Unexpected response while preparing a local backup.");
  }
  return {
    confirmationToken: response.confirmation_token,
    expiresAt: response.expires_at,
    summaryText: formatSummary(response.summary),
    isEmpty: summaryIsEmpty(response.summary),
  };
}

export function toCreatedBackupModel(
  response: ProtocolResponse,
): CreatedBackupModel {
  if (response.type !== "local_backup_created") {
    throw new TypeError("Unexpected response while creating a local backup.");
  }
  return createdBackup(response.backup);
}

export function toVerifiedBackupModel(
  response: ProtocolResponse,
): VerifiedBackupModel {
  if (response.type !== "local_backup_verified") {
    throw new TypeError("The backup or recovery secret could not be verified.");
  }
  return backupMetadata(response.backup);
}

export function toRestoredBackupModel(
  response: ProtocolResponse,
): RestoredBackupModel {
  if (response.type !== "local_backup_restored") {
    throw new TypeError(
      "The backup could not be restored into this local profile.",
    );
  }
  return {
    backup: backupMetadata(response.backup),
    summaryText: formatSummary(response.restored),
  };
}

function createdBackup(backup: LocalBackup): CreatedBackupModel {
  return {
    backupPath: backup.backup_path,
    createdAt: backup.created_at,
    formatVersion: backup.format_version,
    recoverySecret: backup.recovery_secret,
    summaryText: formatSummary(backup.summary),
  };
}

function backupMetadata(backup: LocalBackupMetadata): VerifiedBackupModel {
  return {
    backupPath: backup.backup_path,
    createdAt: backup.created_at,
    formatVersion: backup.format_version,
    schemaVersion: backup.schema_version,
  };
}

function summaryIsEmpty(summary: LocalDataSummary): boolean {
  return (
    summary.aggregate_batch_count === 0 &&
    summary.aggregate_metric_count === 0 &&
    summary.check_in_count === 0 &&
    summary.context_tag_count === 0 &&
    summary.annotation_count === 0 &&
    summary.annotation_context_tag_count === 0
  );
}

function formatSummary(summary: LocalDataSummary): string {
  return [
    countLabel(
      summary.aggregate_batch_count,
      "aggregate batch",
      "aggregate batches",
    ),
    pluralize(summary.check_in_count, "check-in"),
    pluralize(summary.annotation_count, "annotation"),
  ].join(", ");
}

function countLabel(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

function pluralize(count: number, label: string): string {
  return `${count} ${label}${count === 1 ? "" : "s"}`;
}

function requireValue(value: string, label: string): void {
  if (value.trim().length === 0) {
    throw new TypeError(`A ${label} is required.`);
  }
}
