use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::ErrorKind,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use mindcanary_analytics::{
    BaselineConfig, ChangeDirection, DimensionReadiness, InsightDimension, ReadinessStatus,
    analyze_insights, combine_daily_features,
};
use mindcanary_protocol::{
    AggregateBatch, AnnotationRecord, CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES,
    DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT, DEFAULT_DAILY_TIMELINE_LIMIT, DailyBrowserTimeline,
    DailyCheckInTimeline, DailyOsTimeline, DailyRhythmSummary, DailyTimelineDay,
    DailyTimelineSummary, DesktopEnvironment, ErrorCode, LocalBackup, LocalBackupMetadata,
    LocalDataExport, LocalDataSummary, MAX_FRAME_BYTES, OperatingSystem, PROTOCOL_VERSION,
    PlatformCapabilities, PlatformCapability, PlatformCapabilityId, PlatformCapabilityStatus,
    ProtocolRequest, ProtocolResponse, RhythmChangeDirection, RhythmDimensionReadiness,
    RhythmEvidence, RhythmInsight, RhythmInsightDimension, RhythmReadinessStatus, ServiceStatus,
    SessionType, SignalId, SourceHealth, SourceStatus, SourceType, ValidationError,
};
use mindcanary_storage::{
    DailyBrowserFeatures, DailyCheckInFeatures, DailyOsFeatures, EncryptedStore,
    OsKeyringKeyProvider, StorageError,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

mod os_activity;

pub const USER_SERVICE_NAME: &str = "mindcanaryd.service";
const SOURCE_STALE_AFTER: Duration = Duration::minutes(45);

#[derive(Debug)]
pub struct DaemonState {
    store: Mutex<EncryptedStore>,
    destructive_confirmation: Mutex<Option<DestructiveConfirmation>>,
    os_adapter_status: Mutex<OsAdapterRuntimeStatus>,
    os_lifecycle_status: Mutex<OsLifecycleRuntimeStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OsAdapterRuntimeStatus {
    NotStarted,
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct OsLifecycleRuntimeStatus {
    lock_events: bool,
    sleep_events: bool,
}

#[derive(Debug, Clone)]
enum DestructiveConfirmation {
    ClearAll {
        token: uuid::Uuid,
        expires_at: DateTime<Utc>,
    },
    Export {
        token: uuid::Uuid,
        expires_at: DateTime<Utc>,
    },
    Backup {
        token: uuid::Uuid,
        expires_at: DateTime<Utc>,
    },
    DeleteSignal {
        token: uuid::Uuid,
        expires_at: DateTime<Utc>,
        signal: SignalId,
    },
    DeleteAnnotation {
        token: uuid::Uuid,
        expires_at: DateTime<Utc>,
        annotation_id: uuid::Uuid,
    },
    DeleteLatestCheckIn {
        token: uuid::Uuid,
        expires_at: DateTime<Utc>,
        local_date: String,
        check_in_id: uuid::Uuid,
    },
}

impl DaemonState {
    pub fn new(store: EncryptedStore) -> Self {
        Self {
            store: Mutex::new(store),
            destructive_confirmation: Mutex::new(None),
            os_adapter_status: Mutex::new(OsAdapterRuntimeStatus::NotStarted),
            os_lifecycle_status: Mutex::new(OsLifecycleRuntimeStatus::default()),
        }
    }

    pub fn open(database_path: &Path) -> Result<Self> {
        let store = EncryptedStore::bootstrap(database_path, &OsKeyringKeyProvider)
            .context("open encrypted MindCanary database")?;
        Ok(Self::new(store))
    }

    pub fn handle_request(&self, request: ProtocolRequest, now: DateTime<Utc>) -> ProtocolResponse {
        if let Err(error) = request.validate_at(now) {
            return validation_error_response(&error);
        }

        match request {
            ProtocolRequest::Health { .. } => ProtocolResponse::Health {
                protocol_version: PROTOCOL_VERSION,
                service_version: env!("CARGO_PKG_VERSION").to_owned(),
                status: ServiceStatus::Ready,
            },
            ProtocolRequest::GetSourceStatus { .. } => self.source_status(now),
            ProtocolRequest::IngestAggregate { batch, .. } => self.ingest_aggregate(&batch, now),
            ProtocolRequest::SubmitCheckIn { check_in, .. } => self.submit_check_in(&check_in, now),
            ProtocolRequest::PrepareDeleteLatestCheckIn { local_date, .. } => {
                self.prepare_delete_latest_check_in(&local_date, now)
            }
            ProtocolRequest::DeleteLatestCheckIn {
                local_date,
                confirmation_token,
                ..
            } => self.delete_latest_check_in(&local_date, confirmation_token, now),
            ProtocolRequest::SaveAnnotation { annotation, .. } => self.save_annotation(&annotation),
            ProtocolRequest::PrepareDeleteAnnotation { annotation_id, .. } => {
                self.prepare_delete_annotation(annotation_id, now)
            }
            ProtocolRequest::DeleteAnnotation {
                annotation_id,
                confirmation_token,
                ..
            } => self.delete_annotation(annotation_id, confirmation_token, now),
            ProtocolRequest::GetDailyRhythmInsights { limit, .. } => {
                self.daily_rhythm_insights(limit, now)
            }
            ProtocolRequest::GetDailyTimeline { limit, .. } => self.daily_timeline(limit, now),
            ProtocolRequest::GetCollectionSettings { .. } => self.collection_settings(now),
            ProtocolRequest::GetPlatformCapabilities { .. } => {
                ProtocolResponse::PlatformCapabilities {
                    protocol_version: PROTOCOL_VERSION,
                    capabilities: self.platform_capabilities(),
                }
            }
            ProtocolRequest::SetSignalCollection {
                signal, enabled, ..
            } => self.set_signal_collection(signal, enabled, now),
            ProtocolRequest::PrepareDeleteSignalRecords { signal, .. } => {
                self.prepare_delete_signal_records(signal, now)
            }
            ProtocolRequest::DeleteSignalRecords {
                signal,
                confirmation_token,
                ..
            } => self.delete_signal_records(signal, confirmation_token, now),
            ProtocolRequest::GetLocalDataSummary { .. } => self.local_data_summary(),
            ProtocolRequest::PrepareExportLocalRecords { .. } => {
                self.prepare_export_local_records(now)
            }
            ProtocolRequest::ExportLocalRecords {
                confirmation_token,
                export_directory,
                ..
            } => self.export_local_records(confirmation_token, &export_directory, now),
            ProtocolRequest::PrepareCreateLocalBackup { .. } => {
                self.prepare_create_local_backup(now)
            }
            ProtocolRequest::CreateLocalBackup {
                confirmation_token,
                backup_path,
                ..
            } => self.create_local_backup(confirmation_token, &backup_path, now),
            ProtocolRequest::VerifyLocalBackup {
                backup_path,
                recovery_secret,
                ..
            } => Self::verify_local_backup(&backup_path, &recovery_secret),
            ProtocolRequest::RestoreLocalBackup {
                backup_path,
                recovery_secret,
                ..
            } => self.restore_local_backup(&backup_path, &recovery_secret),
            ProtocolRequest::PrepareClearLocalRecords { .. } => {
                self.prepare_clear_local_records(now)
            }
            ProtocolRequest::ClearLocalRecords {
                confirmation_token, ..
            } => self.clear_local_records(confirmation_token, now),
        }
    }

    fn prepare_export_local_records(&self, now: DateTime<Utc>) -> ProtocolResponse {
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let Ok(summary) = local_data_summary(&store) else {
            return internal_error_response();
        };
        drop(store);

        let Ok(token) = random_confirmation_token() else {
            return internal_error_response();
        };
        let expires_at = now + chrono::Duration::minutes(CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES);
        *confirmation = Some(DestructiveConfirmation::Export { token, expires_at });

        ProtocolResponse::ExportLocalRecordsConfirmation {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token: token,
            expires_at,
            summary,
        }
    }

    fn prepare_create_local_backup(&self, now: DateTime<Utc>) -> ProtocolResponse {
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let Ok(summary) = local_data_summary(&store) else {
            return internal_error_response();
        };
        drop(store);

        let Ok(token) = random_confirmation_token() else {
            return internal_error_response();
        };
        let expires_at = now + Duration::minutes(CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES);
        *confirmation = Some(DestructiveConfirmation::Backup { token, expires_at });
        ProtocolResponse::CreateLocalBackupConfirmation {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token: token,
            expires_at,
            summary,
        }
    }

    fn create_local_backup(
        &self,
        confirmation_token: uuid::Uuid,
        backup_path: &str,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let backup_path = Path::new(backup_path);
        if !backup_path.is_absolute() {
            return invalid_request_response();
        }
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Some(DestructiveConfirmation::Backup { token, expires_at }) = confirmation.clone()
        else {
            return invalid_confirmation_response();
        };
        if token != confirmation_token || expires_at < now {
            return invalid_confirmation_response();
        }

        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let summary = match local_data_summary(&store) {
            Ok(summary) => summary,
            Err(error) => return storage_error_response(&error),
        };
        let encrypted = match store.create_encrypted_backup(backup_path, now) {
            Ok(backup) => backup,
            Err(error) => return storage_error_response(&error),
        };
        *confirmation = None;
        ProtocolResponse::LocalBackupCreated {
            protocol_version: PROTOCOL_VERSION,
            backup: LocalBackup {
                backup_path: encrypted.path.display().to_string(),
                created_at: encrypted.created_at,
                format_version: encrypted.format_version,
                schema_version: encrypted.schema_version,
                recovery_secret: encrypted.recovery_secret,
                summary,
            },
        }
    }

    fn verify_local_backup(backup_path: &str, recovery_secret: &str) -> ProtocolResponse {
        let backup_path = Path::new(backup_path);
        if !backup_path.is_absolute() {
            return invalid_request_response();
        }
        match EncryptedStore::verify_encrypted_backup(backup_path, recovery_secret) {
            Ok(metadata) => ProtocolResponse::LocalBackupVerified {
                protocol_version: PROTOCOL_VERSION,
                backup: LocalBackupMetadata {
                    backup_path: backup_path.display().to_string(),
                    created_at: metadata.created_at,
                    format_version: metadata.format_version,
                    schema_version: metadata.schema_version,
                },
            },
            Err(error) => storage_error_response(&error),
        }
    }

    fn restore_local_backup(&self, backup_path: &str, recovery_secret: &str) -> ProtocolResponse {
        let backup_path = Path::new(backup_path);
        if !backup_path.is_absolute() {
            return invalid_request_response();
        }
        let metadata = match EncryptedStore::verify_encrypted_backup(backup_path, recovery_secret) {
            Ok(metadata) => metadata,
            Err(error) => return storage_error_response(&error),
        };
        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };
        if let Err(error) = store.restore_encrypted_backup(backup_path, recovery_secret) {
            return storage_error_response(&error);
        }
        let restored = match local_data_summary(&store) {
            Ok(summary) => summary,
            Err(error) => return storage_error_response(&error),
        };
        ProtocolResponse::LocalBackupRestored {
            protocol_version: PROTOCOL_VERSION,
            backup: LocalBackupMetadata {
                backup_path: backup_path.display().to_string(),
                created_at: metadata.created_at,
                format_version: metadata.format_version,
                schema_version: metadata.schema_version,
            },
            restored,
        }
    }

    fn export_local_records(
        &self,
        confirmation_token: uuid::Uuid,
        export_directory: &str,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let export_directory = Path::new(export_directory);
        if !export_directory.is_absolute() {
            return ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidRequest,
            };
        }

        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Some(DestructiveConfirmation::Export { token, expires_at }) = confirmation.clone()
        else {
            return invalid_confirmation_response();
        };
        if token != confirmation_token || expires_at < now {
            return invalid_confirmation_response();
        }

        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let Ok(export) = export_local_data(&store, export_directory, now) else {
            return internal_error_response();
        };

        *confirmation = None;
        ProtocolResponse::LocalRecordsExported {
            protocol_version: PROTOCOL_VERSION,
            export,
        }
    }

    fn submit_check_in(
        &self,
        check_in: &mindcanary_protocol::CheckInRecord,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };
        match store.submit_check_in_at(check_in, now) {
            Ok(disposition) => ProtocolResponse::CheckInAcknowledged {
                protocol_version: PROTOCOL_VERSION,
                check_in_id: check_in.check_in_id,
                disposition,
            },
            Err(error) => storage_error_response(&error),
        }
    }

    fn prepare_delete_latest_check_in(
        &self,
        local_date: &str,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let check_in_id = match store.latest_check_in_id_for_local_date(local_date) {
            Ok(Some(check_in_id)) => check_in_id,
            Ok(None) => return invalid_request_response(),
            Err(error) => return storage_error_response(&error),
        };
        drop(store);

        let Ok(token) = random_confirmation_token() else {
            return internal_error_response();
        };
        let expires_at = now + Duration::minutes(CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES);
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        *confirmation = Some(DestructiveConfirmation::DeleteLatestCheckIn {
            token,
            expires_at,
            local_date: local_date.to_owned(),
            check_in_id,
        });

        ProtocolResponse::DeleteLatestCheckInConfirmation {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token: token,
            expires_at,
            local_date: local_date.to_owned(),
            check_in_id,
        }
    }

    fn delete_latest_check_in(
        &self,
        local_date: &str,
        confirmation_token: uuid::Uuid,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Some(DestructiveConfirmation::DeleteLatestCheckIn {
            token,
            expires_at,
            local_date: confirmed_local_date,
            check_in_id,
        }) = confirmation.clone()
        else {
            return invalid_confirmation_response();
        };
        if token != confirmation_token || expires_at < now || confirmed_local_date != local_date {
            return invalid_confirmation_response();
        }

        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };
        match store.delete_check_in(check_in_id) {
            Ok(true) => {
                *confirmation = None;
                ProtocolResponse::CheckInDeleted {
                    protocol_version: PROTOCOL_VERSION,
                    local_date: local_date.to_owned(),
                    check_in_id,
                }
            }
            Ok(false) => invalid_request_response(),
            Err(error) => storage_error_response(&error),
        }
    }

    fn save_annotation(&self, annotation: &AnnotationRecord) -> ProtocolResponse {
        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };
        match store.save_annotation(annotation) {
            Ok(()) => ProtocolResponse::AnnotationSaved {
                protocol_version: PROTOCOL_VERSION,
                annotation_id: annotation.annotation_id,
            },
            Err(error) => storage_error_response(&error),
        }
    }

    fn prepare_delete_annotation(
        &self,
        annotation_id: uuid::Uuid,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let annotation_exists = match store.annotations() {
            Ok(annotations) => annotations
                .iter()
                .any(|annotation| annotation.annotation_id == annotation_id),
            Err(error) => return storage_error_response(&error),
        };
        drop(store);
        if !annotation_exists {
            return ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidRequest,
            };
        }

        let Ok(token) = random_confirmation_token() else {
            return internal_error_response();
        };
        let expires_at = now + Duration::minutes(CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES);
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        *confirmation = Some(DestructiveConfirmation::DeleteAnnotation {
            token,
            expires_at,
            annotation_id,
        });
        ProtocolResponse::DeleteAnnotationConfirmation {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token: token,
            expires_at,
            annotation_id,
        }
    }

    fn delete_annotation(
        &self,
        annotation_id: uuid::Uuid,
        confirmation_token: uuid::Uuid,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Some(DestructiveConfirmation::DeleteAnnotation {
            token,
            expires_at,
            annotation_id: confirmed_annotation_id,
        }) = confirmation.clone()
        else {
            return invalid_confirmation_response();
        };
        if token != confirmation_token
            || expires_at < now
            || confirmed_annotation_id != annotation_id
        {
            return invalid_confirmation_response();
        }

        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };
        match store.delete_annotation(annotation_id) {
            Ok(true) => {
                *confirmation = None;
                ProtocolResponse::AnnotationDeleted {
                    protocol_version: PROTOCOL_VERSION,
                    annotation_id,
                }
            }
            Ok(false) => ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidRequest,
            },
            Err(error) => storage_error_response(&error),
        }
    }

    fn daily_rhythm_insights(&self, limit: Option<u16>, now: DateTime<Utc>) -> ProtocolResponse {
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        daily_rhythm_insights(&store, limit, now)
            .unwrap_or_else(|error| storage_error_response(&error))
    }

    fn daily_timeline(&self, limit: Option<u16>, now: DateTime<Utc>) -> ProtocolResponse {
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        daily_timeline(&store, limit, now).unwrap_or_else(|error| storage_error_response(&error))
    }

    fn collection_settings(&self, now: DateTime<Utc>) -> ProtocolResponse {
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        match store.collection_settings(now) {
            Ok(settings) => ProtocolResponse::CollectionSettings {
                protocol_version: PROTOCOL_VERSION,
                settings,
            },
            Err(error) => storage_error_response(&error),
        }
    }

    fn source_status(&self, now: DateTime<Utc>) -> ProtocolResponse {
        let capabilities = self.platform_capabilities();
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let statuses = match (
            store.source_activity_timestamps(),
            store.collection_settings(now),
        ) {
            (Ok(activity), Ok(settings)) => {
                source_statuses(activity, &settings, &capabilities, now)
            }
            (Err(error), _) | (_, Err(error)) => return storage_error_response(&error),
        };

        ProtocolResponse::SourceStatus {
            protocol_version: PROTOCOL_VERSION,
            generated_at: now,
            sources: statuses,
        }
    }

    fn set_signal_collection(
        &self,
        signal: SignalId,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };
        if let Err(error) = store.set_signal_collection(signal, enabled, now) {
            return storage_error_response(&error);
        }
        match store.collection_settings(now) {
            Ok(settings) => ProtocolResponse::CollectionSettings {
                protocol_version: PROTOCOL_VERSION,
                settings,
            },
            Err(error) => storage_error_response(&error),
        }
    }

    fn local_data_summary(&self) -> ProtocolResponse {
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        match local_data_summary(&store) {
            Ok(summary) => ProtocolResponse::LocalDataSummary {
                protocol_version: PROTOCOL_VERSION,
                summary,
            },
            Err(error) => storage_error_response(&error),
        }
    }

    fn ingest_aggregate(&self, batch: &AggregateBatch, now: DateTime<Utc>) -> ProtocolResponse {
        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };

        let mut accepted_batch = batch.clone();
        accepted_batch.metrics.clear();
        for metric in &batch.metrics {
            match store.signal_enabled_for_period(
                metric.signal,
                batch.period.start,
                batch.period.end,
            ) {
                Ok(true) => accepted_batch.metrics.push(metric.clone()),
                Ok(false) => {}
                Err(error) => return storage_error_response(&error),
            }
        }
        if accepted_batch.metrics.is_empty() {
            return ProtocolResponse::IngestAcknowledged {
                protocol_version: PROTOCOL_VERSION,
                batch_id: batch.batch_id,
                disposition: mindcanary_protocol::IngestDisposition::DiscardedDisabled,
            };
        }

        let filtered = accepted_batch.metrics.len() != batch.metrics.len();
        match store.ingest_at(&accepted_batch, now) {
            Ok(mut disposition) => {
                if filtered && disposition == mindcanary_protocol::IngestDisposition::Stored {
                    disposition = mindcanary_protocol::IngestDisposition::StoredFiltered;
                }
                ProtocolResponse::IngestAcknowledged {
                    protocol_version: PROTOCOL_VERSION,
                    batch_id: batch.batch_id,
                    disposition,
                }
            }
            Err(error) => storage_error_response(&error),
        }
    }

    fn platform_capabilities(&self) -> PlatformCapabilities {
        let status = self
            .os_adapter_status
            .lock()
            .map_or(OsAdapterRuntimeStatus::Unavailable, |status| *status);
        let lifecycle = self
            .os_lifecycle_status
            .lock()
            .map_or(OsLifecycleRuntimeStatus::default(), |status| *status);
        platform_capabilities_with_status(PlatformEnvironment::from_process(), status, lifecycle)
    }

    fn prepare_clear_local_records(&self, now: DateTime<Utc>) -> ProtocolResponse {
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let Ok(summary) = local_data_summary(&store) else {
            return internal_error_response();
        };
        drop(store);

        let Ok(token) = random_confirmation_token() else {
            return internal_error_response();
        };
        let expires_at = now + chrono::Duration::minutes(CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES);
        *confirmation = Some(DestructiveConfirmation::ClearAll { token, expires_at });

        ProtocolResponse::ClearLocalRecordsConfirmation {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token: token,
            expires_at,
            summary,
        }
    }

    fn clear_local_records(
        &self,
        confirmation_token: uuid::Uuid,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Some(DestructiveConfirmation::ClearAll { token, expires_at }) = confirmation.clone()
        else {
            return invalid_confirmation_response();
        };
        if token != confirmation_token || expires_at < now {
            return invalid_confirmation_response();
        }

        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };
        let Ok(deleted) = local_data_summary(&store) else {
            return internal_error_response();
        };
        if let Err(error) = store.clear_all() {
            return storage_error_response(&error);
        }

        *confirmation = None;
        ProtocolResponse::LocalRecordsCleared {
            protocol_version: PROTOCOL_VERSION,
            deleted,
        }
    }

    fn prepare_delete_signal_records(
        &self,
        signal: SignalId,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Ok(store) = self.store.lock() else {
            return internal_error_response();
        };
        let Ok(summary) = store.signal_record_summary(signal) else {
            return internal_error_response();
        };
        drop(store);

        let Ok(token) = random_confirmation_token() else {
            return internal_error_response();
        };
        let expires_at = now + chrono::Duration::minutes(CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES);
        *confirmation = Some(DestructiveConfirmation::DeleteSignal {
            token,
            expires_at,
            signal,
        });

        ProtocolResponse::DeleteSignalRecordsConfirmation {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token: token,
            expires_at,
            signal,
            summary,
        }
    }

    fn delete_signal_records(
        &self,
        signal: SignalId,
        confirmation_token: uuid::Uuid,
        now: DateTime<Utc>,
    ) -> ProtocolResponse {
        let Ok(mut confirmation) = self.destructive_confirmation.lock() else {
            return internal_error_response();
        };
        let Some(DestructiveConfirmation::DeleteSignal {
            token,
            expires_at,
            signal: confirmed_signal,
        }) = confirmation.clone()
        else {
            return invalid_confirmation_response();
        };
        if token != confirmation_token || expires_at < now || confirmed_signal != signal {
            return invalid_confirmation_response();
        }

        let Ok(mut store) = self.store.lock() else {
            return internal_error_response();
        };
        let deleted = match store.delete_signal_records(signal) {
            Ok(deleted) => deleted,
            Err(error) => return storage_error_response(&error),
        };

        *confirmation = None;
        ProtocolResponse::SignalRecordsDeleted {
            protocol_version: PROTOCOL_VERSION,
            signal,
            deleted,
        }
    }
}

fn random_confirmation_token() -> Result<uuid::Uuid, getrandom::Error> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)?;
    Ok(uuid::Uuid::from_bytes(bytes))
}

fn source_statuses(
    activity: mindcanary_storage::SourceActivityTimestamps,
    settings: &[mindcanary_protocol::SignalCollectionSetting],
    capabilities: &PlatformCapabilities,
    now: DateTime<Utc>,
) -> Vec<SourceStatus> {
    let browser_enabled = source_has_enabled_signals(settings, SourceType::Browser);
    let os_enabled = source_has_enabled_signals(settings, SourceType::Os);
    let os_available = capabilities.capabilities.iter().any(|capability| {
        capability.capability == PlatformCapabilityId::OsActiveIdleDuration
            && capability.status == PlatformCapabilityStatus::Available
    });

    vec![
        collector_source_status(
            SourceType::Browser,
            browser_enabled,
            true,
            activity.browser,
            now,
        ),
        collector_source_status(SourceType::Os, os_enabled, os_available, activity.os, now),
        SourceStatus {
            source: SourceType::CheckIn,
            health: SourceHealth::Active,
            last_received_at: activity.check_in,
        },
    ]
}

fn source_has_enabled_signals(
    settings: &[mindcanary_protocol::SignalCollectionSetting],
    source: SourceType,
) -> bool {
    settings
        .iter()
        .any(|setting| setting.enabled && setting.signal.source_type() == source)
}

fn collector_source_status(
    source: SourceType,
    enabled: bool,
    available: bool,
    last_received_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> SourceStatus {
    let health = if !available {
        SourceHealth::Unavailable
    } else if !enabled {
        SourceHealth::Disabled
    } else {
        match last_received_at {
            None => SourceHealth::NeverSeen,
            Some(received_at) if now - received_at > SOURCE_STALE_AFTER => SourceHealth::Stale,
            Some(_) => SourceHealth::Active,
        }
    };

    SourceStatus {
        source,
        health,
        last_received_at,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlatformEnvironment {
    operating_system: OperatingSystem,
    desktop_environment: DesktopEnvironment,
    session_type: SessionType,
}

impl PlatformEnvironment {
    fn from_process() -> Self {
        Self {
            operating_system: current_operating_system(),
            desktop_environment: current_desktop_environment(),
            session_type: current_session_type(),
        }
    }
}

#[cfg(test)]
fn platform_capabilities_from(
    environment: PlatformEnvironment,
    runtime_status: OsAdapterRuntimeStatus,
) -> PlatformCapabilities {
    platform_capabilities_with_status(
        environment,
        runtime_status,
        OsLifecycleRuntimeStatus::default(),
    )
}

fn platform_capabilities_with_status(
    environment: PlatformEnvironment,
    runtime_status: OsAdapterRuntimeStatus,
    lifecycle_status: OsLifecycleRuntimeStatus,
) -> PlatformCapabilities {
    let environment_status = os_session_capability_status(environment);
    let session_status = os_lifecycle_capability_status(environment, lifecycle_status);
    let activity_status = os_activity_capability_status(environment, runtime_status);
    PlatformCapabilities {
        operating_system: environment.operating_system,
        desktop_environment: environment.desktop_environment,
        session_type: environment.session_type,
        capabilities: vec![
            PlatformCapability {
                capability: PlatformCapabilityId::OsLockAndSessionEvents,
                status: session_status.0,
                detail: session_status.1.to_owned(),
            },
            PlatformCapability {
                capability: PlatformCapabilityId::OsActiveIdleDuration,
                status: activity_status.0,
                detail: activity_status.1.to_owned(),
            },
            PlatformCapability {
                capability: PlatformCapabilityId::ForegroundApplicationCategory,
                status: if environment_status.0 == PlatformCapabilityStatus::Planned {
                    PlatformCapabilityStatus::Planned
                } else {
                    PlatformCapabilityStatus::Unavailable
                },
                detail: if environment_status.0 == PlatformCapabilityStatus::Planned {
                    "GNOME/X11 environment detected; foreground categories still require a separate opt-in adapter."
                } else {
                    environment_status.1
                }
                .to_owned(),
            },
        ],
    }
}

fn os_lifecycle_capability_status(
    environment: PlatformEnvironment,
    runtime_status: OsLifecycleRuntimeStatus,
) -> (PlatformCapabilityStatus, &'static str) {
    let base = os_session_capability_status(environment);
    if base.0 == PlatformCapabilityStatus::Unavailable {
        return base;
    }
    match (runtime_status.lock_events, runtime_status.sleep_events) {
        (true, true) => (
            PlatformCapabilityStatus::Available,
            "GNOME lock events and Linux suspend/resume events are available; each signal remains off until explicitly enabled.",
        ),
        (true, false) => (
            PlatformCapabilityStatus::Planned,
            "GNOME lock events are available, but Linux suspend/resume events are unavailable right now.",
        ),
        (false, true) => (
            PlatformCapabilityStatus::Planned,
            "Linux suspend/resume events are available, but GNOME lock events are unavailable right now.",
        ),
        (false, false) => (
            PlatformCapabilityStatus::Planned,
            "GNOME/X11 environment detected; lifecycle event adapters have not connected in this process.",
        ),
    }
}

fn os_session_capability_status(
    environment: PlatformEnvironment,
) -> (PlatformCapabilityStatus, &'static str) {
    if environment.operating_system != OperatingSystem::Linux {
        return (
            PlatformCapabilityStatus::Unavailable,
            "OS activity adapters are not implemented for this operating system.",
        );
    }

    match (environment.desktop_environment, environment.session_type) {
        (DesktopEnvironment::Gnome, SessionType::X11) => (
            PlatformCapabilityStatus::Planned,
            "GNOME/X11 environment detected; lock and session events still require a separate adapter.",
        ),
        (_, SessionType::Wayland) => (
            PlatformCapabilityStatus::Unavailable,
            "Wayland support requires a separate adapter and is unavailable in this build.",
        ),
        _ => (
            PlatformCapabilityStatus::Unavailable,
            "Only the first GNOME/X11 Linux adapter is planned for this build.",
        ),
    }
}

fn os_activity_capability_status(
    environment: PlatformEnvironment,
    runtime_status: OsAdapterRuntimeStatus,
) -> (PlatformCapabilityStatus, &'static str) {
    let base = os_session_capability_status(environment);
    if base.0 == PlatformCapabilityStatus::Unavailable {
        return base;
    }

    match runtime_status {
        OsAdapterRuntimeStatus::NotStarted => (
            PlatformCapabilityStatus::Planned,
            "GNOME/X11 environment detected; the idle-time adapter has not started in this process.",
        ),
        OsAdapterRuntimeStatus::Available => (
            PlatformCapabilityStatus::Available,
            "GNOME/X11 idle-time adapter is available; collection remains off until explicitly enabled.",
        ),
        OsAdapterRuntimeStatus::Unavailable => (
            PlatformCapabilityStatus::Unavailable,
            "GNOME/X11 detected, but the local idle-time service is unavailable right now.",
        ),
    }
}

const fn current_operating_system() -> OperatingSystem {
    if cfg!(target_os = "linux") {
        OperatingSystem::Linux
    } else if cfg!(target_os = "macos") {
        OperatingSystem::Macos
    } else if cfg!(target_os = "windows") {
        OperatingSystem::Windows
    } else {
        OperatingSystem::Other
    }
}

fn current_desktop_environment() -> DesktopEnvironment {
    let value = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("DESKTOP_SESSION"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if value.contains("gnome") {
        DesktopEnvironment::Gnome
    } else if value.contains("kde") || value.contains("plasma") {
        DesktopEnvironment::Kde
    } else if value.is_empty() {
        DesktopEnvironment::Unknown
    } else {
        DesktopEnvironment::Other
    }
}

fn current_session_type() -> SessionType {
    match std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "x11" => SessionType::X11,
        "wayland" => SessionType::Wayland,
        "" => SessionType::Unknown,
        _ => SessionType::Other,
    }
}

fn local_data_summary(store: &EncryptedStore) -> Result<LocalDataSummary, StorageError> {
    Ok(LocalDataSummary {
        aggregate_batch_count: store.aggregate_batch_count()?,
        aggregate_metric_count: store.metric_count()?,
        check_in_count: store.check_in_count()?,
        context_tag_count: store.check_in_context_tag_count()?,
        annotation_count: store.annotation_count()?,
        annotation_context_tag_count: store.annotation_context_tag_count()?,
    })
}

fn export_local_data(
    store: &EncryptedStore,
    export_directory: &Path,
    generated_at: DateTime<Utc>,
) -> Result<LocalDataExport> {
    fs::create_dir_all(export_directory).context("create local export directory")?;
    #[cfg(unix)]
    fs::set_permissions(export_directory, fs::Permissions::from_mode(0o700))
        .context("set local export directory permissions")?;

    let summary = local_data_summary(store).context("summarize local data for export")?;
    let browser = store
        .daily_browser_features()
        .context("read browser features for export")?;
    let os = store
        .daily_os_features()
        .context("read OS features for export")?;
    let check_ins = store
        .daily_check_in_features()
        .context("read check-in features for export")?;
    let annotations = store.annotations().context("read annotations for export")?;

    let report_path = export_directory.join("mindcanary-report.md");
    let daily_browser_csv_path = export_directory.join("daily-browser.csv");
    let daily_os_csv_path = export_directory.join("daily-os.csv");
    let daily_check_in_csv_path = export_directory.join("daily-check-ins.csv");
    let annotations_csv_path = export_directory.join("annotations.csv");

    fs::write(
        &report_path,
        export_report(
            summary,
            &browser,
            &os,
            &check_ins,
            &annotations,
            generated_at,
        ),
    )
    .context("write local export report")?;
    fs::write(&daily_browser_csv_path, daily_browser_csv(&browser))
        .context("write browser CSV export")?;
    fs::write(&daily_os_csv_path, daily_os_csv(&os)).context("write OS CSV export")?;
    fs::write(&daily_check_in_csv_path, daily_check_in_csv(&check_ins))
        .context("write check-in CSV export")?;
    fs::write(&annotations_csv_path, annotations_csv(&annotations))
        .context("write annotation CSV export")?;

    Ok(LocalDataExport {
        export_directory: export_directory.display().to_string(),
        report_path: report_path.display().to_string(),
        daily_browser_csv_path: daily_browser_csv_path.display().to_string(),
        daily_os_csv_path: daily_os_csv_path.display().to_string(),
        daily_check_in_csv_path: daily_check_in_csv_path.display().to_string(),
        annotations_csv_path: annotations_csv_path.display().to_string(),
        summary,
    })
}

fn export_report(
    summary: LocalDataSummary,
    browser: &[DailyBrowserFeatures],
    os: &[DailyOsFeatures],
    check_ins: &[DailyCheckInFeatures],
    annotations: &[AnnotationRecord],
    generated_at: DateTime<Utc>,
) -> String {
    let mut report = String::new();
    report.push_str("# MindCanary Local Export\n\n");
    let _ = writeln!(report, "Generated at: {}\n", generated_at.to_rfc3339());
    report.push_str("This export stays on this device unless you move it. It contains daily aggregates, check-in summaries, and the private annotations you wrote. It does not contain URLs, page titles, page text, keystrokes, diagnoses, or clinical phase labels.\n\n");
    report.push_str("## Counts\n\n");
    let _ = writeln!(
        report,
        "- Aggregate batches: {}\n- Aggregate metric records: {}\n- Check-ins: {}\n- Check-in context tags: {}\n- Annotations: {}\n- Annotation context tags: {}\n\n",
        summary.aggregate_batch_count,
        summary.aggregate_metric_count,
        summary.check_in_count,
        summary.context_tag_count,
        summary.annotation_count,
        summary.annotation_context_tag_count
    );
    report.push_str("## Files\n\n");
    report.push_str("- `daily-browser.csv`: daily browser rhythm aggregates.\n");
    report.push_str("- `daily-os.csv`: daily operating-system activity aggregates.\n");
    report.push_str("- `daily-check-ins.csv`: daily self-report summaries.\n\n");
    report
        .push_str("- `annotations.csv`: private user-written day and time-window annotations.\n\n");
    report.push_str("## Coverage\n\n");
    let _ = writeln!(
        report,
        "- Browser days: {}\n- OS days: {}\n- Check-in days: {}\n- Annotations: {}\n",
        browser.len(),
        os.len(),
        check_ins.len(),
        annotations.len()
    );
    report
}

fn daily_browser_csv(rows: &[DailyBrowserFeatures]) -> String {
    let mut csv = String::from(
        "local_date,open_tab_count_mean,open_tab_count_max,tab_switch_count,retained_across_day_count,continuous_scrolling_seconds,active_seconds,idle_seconds,recorded_bucket_count\n",
    );
    for row in rows {
        csv.push_str(&csv_row([
            row.local_date.as_str(),
            &csv_optional_f64(row.open_tab_count_mean),
            &csv_optional_f64(row.open_tab_count_max),
            &csv_optional_f64(row.tab_switch_count),
            &csv_optional_f64(row.retained_across_day_count),
            &csv_optional_f64(row.continuous_scrolling_seconds),
            &csv_optional_f64(row.active_seconds),
            &csv_optional_f64(row.idle_seconds),
            &row.aggregate_bucket_count.to_string(),
        ]));
    }
    csv
}

fn daily_os_csv(rows: &[DailyOsFeatures]) -> String {
    let mut csv = String::from(
        "local_date,active_seconds,idle_seconds,lock_count,unlock_count,suspend_count,resume_count,recorded_bucket_count\n",
    );
    for row in rows {
        csv.push_str(&csv_row([
            row.local_date.as_str(),
            &csv_optional_f64(row.active_seconds),
            &csv_optional_f64(row.idle_seconds),
            &csv_optional_f64(row.lock_count),
            &csv_optional_f64(row.unlock_count),
            &csv_optional_f64(row.suspend_count),
            &csv_optional_f64(row.resume_count),
            &row.aggregate_bucket_count.to_string(),
        ]));
    }
    csv
}

fn daily_check_in_csv(rows: &[DailyCheckInFeatures]) -> String {
    let mut csv = String::from(
        "local_date,sleep_minutes,mood,energy,irritability,concentration,impulsivity,check_in_count,context_tags\n",
    );
    for row in rows {
        let context_tags = row
            .context_tags
            .iter()
            .map(|tag| tag.as_str())
            .collect::<Vec<_>>()
            .join(";");
        csv.push_str(&csv_row([
            row.local_date.as_str(),
            &csv_optional_f64(row.sleep_minutes),
            &csv_optional_f64(row.mood),
            &csv_optional_f64(row.energy),
            &csv_optional_f64(row.irritability),
            &csv_optional_f64(row.concentration),
            &csv_optional_f64(row.impulsivity),
            &row.check_in_count.to_string(),
            &context_tags,
        ]));
    }
    csv
}

fn annotations_csv(rows: &[AnnotationRecord]) -> String {
    let mut csv = String::from(
        "annotation_id,created_at,time_zone,local_date,start_minute,end_minute,context_tags,text\n",
    );
    for row in rows {
        let context_tags = row
            .context_tags
            .iter()
            .map(|tag| tag.as_str())
            .collect::<Vec<_>>()
            .join(";");
        csv.push_str(&csv_row([
            &row.annotation_id.to_string(),
            &row.created_at.to_rfc3339(),
            row.time_zone.as_str(),
            row.local_date.as_str(),
            &row.start_minute
                .map_or_else(String::new, |value| value.to_string()),
            &row.end_minute
                .map_or_else(String::new, |value| value.to_string()),
            &context_tags,
            row.text.as_str(),
        ]));
    }
    csv
}

fn csv_optional_f64(value: Option<f64>) -> String {
    value.map_or_else(String::new, |value| format!("{value:.3}"))
}

fn csv_row<const N: usize>(fields: [&str; N]) -> String {
    let mut row = fields.map(csv_field).join(",");
    row.push('\n');
    row
}

fn csv_field(value: &str) -> String {
    let mut value = value.to_owned();
    if value
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '=' | '+' | '-' | '@' | '\t' | '\r'))
    {
        value.insert(0, '\'');
    }
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

fn internal_error_response() -> ProtocolResponse {
    ProtocolResponse::Error {
        protocol_version: PROTOCOL_VERSION,
        code: ErrorCode::Internal,
    }
}

fn invalid_confirmation_response() -> ProtocolResponse {
    ProtocolResponse::Error {
        protocol_version: PROTOCOL_VERSION,
        code: ErrorCode::InvalidConfirmation,
    }
}

fn invalid_request_response() -> ProtocolResponse {
    ProtocolResponse::Error {
        protocol_version: PROTOCOL_VERSION,
        code: ErrorCode::InvalidRequest,
    }
}

fn daily_rhythm_insights(
    store: &EncryptedStore,
    limit: Option<u16>,
    generated_at: DateTime<Utc>,
) -> Result<ProtocolResponse, StorageError> {
    let browser = store.daily_browser_features()?;
    let os = store.daily_os_features()?;
    let check_ins = store.daily_check_in_features()?;
    let snapshots = combine_daily_features(&browser, &os, &check_ins);
    let analysis = analyze_insights(&snapshots, BaselineConfig::default());
    let limit = usize::from(limit.unwrap_or(DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT));
    let insight_count_before_limit = analysis.insights.len();
    let insights_truncated = insight_count_before_limit > limit;
    let insights = analysis
        .insights
        .into_iter()
        .take(limit)
        .map(protocol_insight)
        .collect::<Vec<_>>();
    let readiness = analysis.readiness.iter().map(protocol_readiness).collect();

    Ok(ProtocolResponse::DailyRhythmInsights {
        protocol_version: PROTOCOL_VERSION,
        generated_at,
        summary: DailyRhythmSummary {
            daily_snapshot_count: snapshots.len(),
            browser_day_count: browser.len(),
            os_day_count: os.len(),
            check_in_day_count: check_ins.len(),
            insight_count_before_limit,
            insights_truncated,
        },
        readiness,
        insights,
    })
}

fn daily_timeline(
    store: &EncryptedStore,
    limit: Option<u16>,
    generated_at: DateTime<Utc>,
) -> Result<ProtocolResponse, StorageError> {
    let mut browser = dated_browser_features(store.daily_browser_features()?)?;
    let mut os = dated_os_features(store.daily_os_features()?)?;
    let mut check_ins = dated_check_in_features(store.daily_check_in_features()?)?;
    let mut annotations = dated_annotations(store.annotations()?)?;
    let first_date = browser
        .keys()
        .chain(os.keys())
        .chain(check_ins.keys())
        .chain(annotations.keys())
        .min()
        .copied();
    let last_date = browser
        .keys()
        .chain(os.keys())
        .chain(check_ins.keys())
        .chain(annotations.keys())
        .max()
        .copied();

    let (Some(first_date), Some(last_date)) = (first_date, last_date) else {
        return Ok(ProtocolResponse::DailyTimeline {
            protocol_version: PROTOCOL_VERSION,
            generated_at,
            summary: DailyTimelineSummary {
                calendar_day_count_before_limit: 0,
                returned_day_count: 0,
                browser_day_count: 0,
                os_day_count: 0,
                check_in_day_count: 0,
                annotation_day_count: 0,
                missing_day_count: 0,
                days_truncated: false,
            },
            days: Vec::new(),
        });
    };

    let calendar_day_count_before_limit = usize::try_from((last_date - first_date).num_days() + 1)
        .map_err(|_| StorageError::InvalidStoredCount)?;
    let limit = usize::from(limit.unwrap_or(DEFAULT_DAILY_TIMELINE_LIMIT));
    let offset =
        i64::try_from(limit.saturating_sub(1)).map_err(|_| StorageError::InvalidStoredCount)?;
    let limited_first_date = last_date
        .checked_sub_signed(Duration::days(offset))
        .map_or(first_date, |date| date.max(first_date));

    let mut days = Vec::with_capacity(limit.min(calendar_day_count_before_limit));
    let mut date = limited_first_date;
    loop {
        let browser = browser.remove(&date).as_ref().map(protocol_browser_day);
        let os = os.remove(&date).as_ref().map(protocol_os_day);
        let check_in = check_ins.remove(&date).map(protocol_check_in_day);
        let annotations = annotations.remove(&date).unwrap_or_default();
        days.push(DailyTimelineDay {
            local_date: date.to_string(),
            browser,
            os,
            check_in,
            annotations,
        });
        if date == last_date {
            break;
        }
        date = date
            .succ_opt()
            .ok_or_else(|| StorageError::InvalidStoredLocalDate(date.to_string()))?;
    }

    let browser_day_count = days.iter().filter(|day| day.browser.is_some()).count();
    let os_day_count = days.iter().filter(|day| day.os.is_some()).count();
    let check_in_day_count = days.iter().filter(|day| day.check_in.is_some()).count();
    let annotation_day_count = days
        .iter()
        .filter(|day| !day.annotations.is_empty())
        .count();
    let missing_day_count = days
        .iter()
        .filter(|day| {
            day.browser.is_none()
                && day.os.is_none()
                && day.check_in.is_none()
                && day.annotations.is_empty()
        })
        .count();

    Ok(ProtocolResponse::DailyTimeline {
        protocol_version: PROTOCOL_VERSION,
        generated_at,
        summary: DailyTimelineSummary {
            calendar_day_count_before_limit,
            returned_day_count: days.len(),
            browser_day_count,
            os_day_count,
            check_in_day_count,
            annotation_day_count,
            missing_day_count,
            days_truncated: calendar_day_count_before_limit > days.len(),
        },
        days,
    })
}

fn dated_browser_features(
    features: Vec<DailyBrowserFeatures>,
) -> Result<BTreeMap<NaiveDate, DailyBrowserFeatures>, StorageError> {
    features
        .into_iter()
        .map(|features| {
            let date = stored_local_date(&features.local_date)?;
            Ok((date, features))
        })
        .collect()
}

fn dated_os_features(
    features: Vec<DailyOsFeatures>,
) -> Result<BTreeMap<NaiveDate, DailyOsFeatures>, StorageError> {
    features
        .into_iter()
        .map(|features| {
            let date = stored_local_date(&features.local_date)?;
            Ok((date, features))
        })
        .collect()
}

fn dated_check_in_features(
    features: Vec<DailyCheckInFeatures>,
) -> Result<BTreeMap<NaiveDate, DailyCheckInFeatures>, StorageError> {
    features
        .into_iter()
        .map(|features| {
            let date = stored_local_date(&features.local_date)?;
            Ok((date, features))
        })
        .collect()
}

fn dated_annotations(
    annotations: Vec<AnnotationRecord>,
) -> Result<BTreeMap<NaiveDate, Vec<AnnotationRecord>>, StorageError> {
    let mut dated = BTreeMap::<NaiveDate, Vec<AnnotationRecord>>::new();
    for annotation in annotations {
        let date = stored_local_date(&annotation.local_date)?;
        dated.entry(date).or_default().push(annotation);
    }
    Ok(dated)
}

fn stored_local_date(value: &str) -> Result<NaiveDate, StorageError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| StorageError::InvalidStoredLocalDate(value.to_owned()))
}

fn protocol_browser_day(features: &DailyBrowserFeatures) -> DailyBrowserTimeline {
    DailyBrowserTimeline {
        open_tab_count_mean: features.open_tab_count_mean,
        open_tab_count_max: features.open_tab_count_max,
        tab_switch_count: features.tab_switch_count,
        retained_across_day_count: features.retained_across_day_count,
        continuous_scrolling_seconds: features.continuous_scrolling_seconds,
        active_seconds: features.active_seconds,
        idle_seconds: features.idle_seconds,
        recorded_bucket_count: features.aggregate_bucket_count,
    }
}

fn protocol_os_day(features: &DailyOsFeatures) -> DailyOsTimeline {
    DailyOsTimeline {
        active_seconds: features.active_seconds,
        idle_seconds: features.idle_seconds,
        lock_count: features.lock_count,
        unlock_count: features.unlock_count,
        suspend_count: features.suspend_count,
        resume_count: features.resume_count,
        recorded_bucket_count: features.aggregate_bucket_count,
    }
}

fn protocol_check_in_day(features: DailyCheckInFeatures) -> DailyCheckInTimeline {
    DailyCheckInTimeline {
        sleep_minutes: features.sleep_minutes,
        mood: features.mood,
        energy: features.energy,
        irritability: features.irritability,
        concentration: features.concentration,
        impulsivity: features.impulsivity,
        check_in_count: features.check_in_count,
        context_tags: features.context_tags,
    }
}

fn protocol_insight(insight: mindcanary_analytics::Insight) -> RhythmInsight {
    RhythmInsight {
        local_date: insight.local_date,
        dimension: protocol_dimension(insight.dimension),
        direction: protocol_direction(insight.direction),
        summary: insight.summary,
        evidence: insight
            .evidence
            .into_iter()
            .map(|evidence| RhythmEvidence {
                label: evidence.label,
                value: evidence.value,
            })
            .collect(),
    }
}

fn protocol_readiness(readiness: &DimensionReadiness) -> RhythmDimensionReadiness {
    RhythmDimensionReadiness {
        dimension: protocol_dimension(readiness.dimension),
        status: protocol_readiness_status(readiness.status),
        comparable_day_count: readiness.comparable_day_count,
        minimum_day_count: readiness.minimum_day_count,
    }
}

const fn protocol_dimension(dimension: InsightDimension) -> RhythmInsightDimension {
    match dimension {
        InsightDimension::BrowserTabs => RhythmInsightDimension::BrowserTabs,
        InsightDimension::TabSwitching => RhythmInsightDimension::TabSwitching,
        InsightDimension::ActiveTime => RhythmInsightDimension::ActiveTime,
        InsightDimension::ComputerActiveTime => RhythmInsightDimension::ComputerActiveTime,
        InsightDimension::Sleep => RhythmInsightDimension::Sleep,
        InsightDimension::Energy => RhythmInsightDimension::Energy,
    }
}

const fn protocol_direction(direction: ChangeDirection) -> RhythmChangeDirection {
    match direction {
        ChangeDirection::Higher => RhythmChangeDirection::Higher,
        ChangeDirection::Lower => RhythmChangeDirection::Lower,
    }
}

const fn protocol_readiness_status(status: ReadinessStatus) -> RhythmReadinessStatus {
    match status {
        ReadinessStatus::ChangeDescribed => RhythmReadinessStatus::ChangeDescribed,
        ReadinessStatus::WithinBaseline => RhythmReadinessStatus::WithinBaseline,
        ReadinessStatus::NeedsSustainedChange => RhythmReadinessStatus::NeedsSustainedChange,
        ReadinessStatus::MissingCurrent => RhythmReadinessStatus::MissingCurrent,
        ReadinessStatus::InsufficientBaseline => RhythmReadinessStatus::InsufficientBaseline,
        ReadinessStatus::ZeroBaseline => RhythmReadinessStatus::ZeroBaseline,
        ReadinessStatus::UnstableBaseline => RhythmReadinessStatus::UnstableBaseline,
    }
}

fn validation_error_response(error: &ValidationError) -> ProtocolResponse {
    let code = match error {
        ValidationError::UnsupportedProtocolVersion { .. } => ErrorCode::UnsupportedProtocolVersion,
        _ => ErrorCode::InvalidRequest,
    };

    ProtocolResponse::Error {
        protocol_version: PROTOCOL_VERSION,
        code,
    }
}

fn storage_error_response(error: &StorageError) -> ProtocolResponse {
    let code = match error {
        StorageError::SequenceConflict => ErrorCode::SequenceConflict,
        StorageError::BackupPathNotAbsolute
        | StorageError::BackupParentMissing
        | StorageError::BackupAlreadyExists
        | StorageError::InvalidRecoverySecret
        | StorageError::InvalidBackupKey(_)
        | StorageError::InvalidBackupFormat
        | StorageError::BackupIntegrityFailed
        | StorageError::InvalidBackupTimestamp
        | StorageError::UnsupportedBackupFormat { .. }
        | StorageError::UnsupportedBackupSchema { .. }
        | StorageError::RestoreRequiresEmptyRecords => ErrorCode::InvalidRequest,
        _ => ErrorCode::Internal,
    };

    ProtocolResponse::Error {
        protocol_version: PROTOCOL_VERSION,
        code,
    }
}

pub fn default_socket_path() -> PathBuf {
    let runtime_root =
        std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    runtime_root.join("mindcanary").join("mindcanaryd.sock")
}

pub fn user_service_dir() -> Result<PathBuf> {
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    user_service_dir_from_values(xdg_config_home, home)
}

pub fn user_service_dir_from_values(
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf> {
    let config_home = match xdg_config_home.filter(|path| !path.as_os_str().is_empty()) {
        Some(path) => path,
        None => home
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| path.join(".config"))
            .context("HOME or XDG_CONFIG_HOME is required to install the user service")?,
    };

    Ok(config_home.join("systemd").join("user"))
}

pub fn systemd_user_service_unit(daemon_path: &Path) -> Result<String> {
    if !daemon_path.is_absolute() {
        anyhow::bail!("systemd user service requires an absolute daemon path");
    }
    let daemon_path = daemon_path
        .to_str()
        .context("daemon path must be valid UTF-8")?;
    let exec_start = quote_systemd_exec_argument(daemon_path)?;

    Ok(format!(
        "\
[Unit]
Description=MindCanary local data daemon
Documentation=https://github.com/mindcanary/mindcanary

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
"
    ))
}

pub fn install_user_service(
    daemon_path: &Path,
    service_dir: Option<&Path>,
    enable_now: bool,
) -> Result<PathBuf> {
    let unit = systemd_user_service_unit(daemon_path)?;
    let service_dir = service_dir
        .map(Path::to_path_buf)
        .map_or_else(user_service_dir, Ok)?;

    fs::create_dir_all(&service_dir).context("create systemd user service directory")?;
    let service_path = service_dir.join(USER_SERVICE_NAME);
    fs::write(&service_path, unit).context("write systemd user service unit")?;

    #[cfg(unix)]
    fs::set_permissions(&service_path, fs::Permissions::from_mode(0o644))
        .context("set systemd user service permissions")?;

    if enable_now {
        run_systemctl_user(["daemon-reload"])?;
        run_systemctl_user(["enable", USER_SERVICE_NAME])?;
        run_systemctl_user(["restart", USER_SERVICE_NAME])?;
    }

    Ok(service_path)
}

pub fn uninstall_user_service(service_dir: Option<&Path>, disable_now: bool) -> Result<PathBuf> {
    let service_dir = service_dir
        .map(Path::to_path_buf)
        .map_or_else(user_service_dir, Ok)?;
    let service_path = service_dir.join(USER_SERVICE_NAME);

    if disable_now {
        run_systemctl_user(["disable", "--now", USER_SERVICE_NAME])?;
    }

    match fs::remove_file(&service_path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("remove systemd user service unit"),
    }

    if disable_now {
        run_systemctl_user(["daemon-reload"])?;
    }

    Ok(service_path)
}

fn quote_systemd_exec_argument(value: &str) -> Result<String> {
    if value.is_empty() || value.contains('\n') || value.contains('\0') {
        anyhow::bail!("systemd executable path contains unsupported characters");
    }

    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '\\' | '"' | '$' | '`' => {
                quoted.push('\\');
                quoted.push(character);
            }
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    Ok(quoted)
}

fn run_systemctl_user<const N: usize>(args: [&str; N]) -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .context("run systemctl --user")?;

    if !status.success() {
        anyhow::bail!("systemctl --user exited with status {status}");
    }

    Ok(())
}

pub async fn run(socket_path: &Path, database_path: &Path) -> Result<()> {
    let state = std::sync::Arc::new(DaemonState::open(database_path)?);
    if PlatformEnvironment::from_process()
        == (PlatformEnvironment {
            operating_system: OperatingSystem::Linux,
            desktop_environment: DesktopEnvironment::Gnome,
            session_type: SessionType::X11,
        })
    {
        tokio::spawn(os_activity::run(std::sync::Arc::clone(&state)));
    }
    run_with_state(socket_path, state).await
}

pub async fn run_with_state(socket_path: &Path, state: std::sync::Arc<DaemonState>) -> Result<()> {
    prepare_socket_parent(socket_path)?;

    match std::fs::remove_file(socket_path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("remove stale daemon socket"),
    }

    let listener = UnixListener::bind(socket_path).context("bind daemon socket")?;
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))
        .context("set daemon socket permissions")?;

    loop {
        let (stream, _) = listener.accept().await.context("accept local client")?;
        let state = std::sync::Arc::clone(&state);

        tokio::spawn(async move {
            if handle_connection(stream, &state).await.is_err() {
                eprintln!("local_client_error");
            }
        });
    }
}

fn prepare_socket_parent(socket_path: &Path) -> Result<()> {
    let parent = socket_path
        .parent()
        .context("daemon socket path must have a parent directory")?;
    std::fs::create_dir_all(parent).context("create daemon runtime directory")?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
        .context("set daemon runtime directory permissions")?;
    Ok(())
}

async fn handle_connection(mut stream: UnixStream, state: &DaemonState) -> Result<()> {
    loop {
        let Some(frame) = read_frame(&mut stream).await? else {
            return Ok(());
        };

        let response = match serde_json::from_slice::<ProtocolRequest>(&frame) {
            Ok(request) => state.handle_request(request, Utc::now()),
            Err(_) => ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidRequest,
            },
        };

        write_frame(&mut stream, &serde_json::to_vec(&response)?).await?;
    }
}

async fn read_frame(stream: &mut UnixStream) -> Result<Option<Vec<u8>>> {
    let mut length_bytes = [0_u8; 4];
    match stream.read_exact(&mut length_bytes).await {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error).context("read local frame length"),
    }

    let length = u32::from_be_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        anyhow::bail!("invalid local frame length");
    }

    let mut payload = vec![0_u8; length];
    stream
        .read_exact(&mut payload)
        .await
        .context("read local frame payload")?;
    Ok(Some(payload))
}

async fn write_frame(stream: &mut UnixStream, payload: &[u8]) -> Result<()> {
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        anyhow::bail!("invalid outgoing local frame length");
    }

    let length = u32::try_from(payload.len()).context("frame length exceeds u32")?;
    stream
        .write_all(&length.to_be_bytes())
        .await
        .context("write local frame length")?;
    stream
        .write_all(payload)
        .await
        .context("write local frame payload")?;
    stream.flush().await.context("flush local frame")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    use chrono::{TimeZone, Utc};
    use mindcanary_protocol::{
        AggregateBatch, AnnotationRecord, CheckInRecord, ContextTag, IngestDisposition,
        MAX_DAILY_RHYTHM_INSIGHT_LIMIT, Metric, ObservationPeriod, PROTOCOL_VERSION,
        RhythmInsightDimension, RhythmReadinessStatus, SignalId,
    };
    use uuid::Uuid;

    use super::*;

    fn state(temp: &tempfile::TempDir) -> DaemonState {
        let key = mindcanary_storage::DatabaseKey::from_bytes([42; 32]);
        let store = EncryptedStore::open(temp.path().join("mindcanary.db"), &key).unwrap();
        DaemonState::new(store)
    }

    fn enabled_state(temp: &tempfile::TempDir) -> DaemonState {
        let state = state(temp);
        let enabled_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        {
            let mut store = state.store.lock().unwrap();
            for signal in SignalId::ALL {
                store
                    .set_signal_collection(signal, true, enabled_at)
                    .unwrap();
            }
        }
        state
    }

    #[test]
    fn systemd_user_unit_runs_the_daemon_directly() {
        let unit =
            systemd_user_service_unit(Path::new("/opt/Mind Canary/bin/mindcanaryd")).unwrap();

        assert!(unit.contains("[Unit]\nDescription=MindCanary local data daemon"));
        assert!(unit.contains("ExecStart=\"/opt/Mind Canary/bin/mindcanaryd\""));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("[Install]\nWantedBy=default.target"));
        assert!(!unit.contains("--socket"));
        assert!(!unit.contains("--database"));
    }

    #[test]
    fn systemd_user_unit_rejects_relative_daemon_paths() {
        assert!(systemd_user_service_unit(Path::new("target/debug/mindcanaryd")).is_err());
    }

    #[test]
    fn systemd_user_unit_escapes_special_exec_characters() {
        let unit = systemd_user_service_unit(Path::new("/opt/Mind Canary/bin/mindcanaryd-$stable"))
            .unwrap();

        assert!(unit.contains("ExecStart=\"/opt/Mind Canary/bin/mindcanaryd-\\$stable\""));
    }

    #[test]
    fn user_service_dir_prefers_xdg_config_home() {
        let dir = user_service_dir_from_values(
            Some(PathBuf::from("/tmp/mc-config")),
            Some(PathBuf::from("/home/tester")),
        )
        .unwrap();

        assert_eq!(dir, PathBuf::from("/tmp/mc-config/systemd/user"));
    }

    #[test]
    fn user_service_dir_falls_back_to_home_config() {
        let dir = user_service_dir_from_values(None, Some(PathBuf::from("/home/tester"))).unwrap();

        assert_eq!(dir, PathBuf::from("/home/tester/.config/systemd/user"));
    }

    #[test]
    fn install_user_service_writes_unit_into_requested_directory() {
        let directory = tempfile::TempDir::new().unwrap();
        let service_path = install_user_service(
            Path::new("/opt/mindcanary/bin/mindcanaryd"),
            Some(directory.path()),
            false,
        )
        .unwrap();

        assert_eq!(service_path, directory.path().join(USER_SERVICE_NAME));
        let unit = std::fs::read_to_string(&service_path).unwrap();
        assert!(unit.contains("ExecStart=\"/opt/mindcanary/bin/mindcanaryd\""));

        let service_mode = std::fs::metadata(&service_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(service_mode, 0o644);
    }

    #[test]
    fn uninstall_user_service_removes_requested_unit_file() {
        let directory = tempfile::TempDir::new().unwrap();
        let service_path = install_user_service(
            Path::new("/opt/mindcanary/bin/mindcanaryd"),
            Some(directory.path()),
            false,
        )
        .unwrap();
        assert!(service_path.exists());

        let removed_path = uninstall_user_service(Some(directory.path()), false).unwrap();

        assert_eq!(removed_path, service_path);
        assert!(!removed_path.exists());
    }

    #[test]
    fn uninstall_user_service_is_idempotent_for_missing_unit_file() {
        let directory = tempfile::TempDir::new().unwrap();

        let removed_path = uninstall_user_service(Some(directory.path()), false).unwrap();

        assert_eq!(removed_path, directory.path().join(USER_SERVICE_NAME));
        assert!(!removed_path.exists());
    }

    fn request(source: Uuid, batch: Uuid, sequence: u64) -> ProtocolRequest {
        ProtocolRequest::IngestAggregate {
            protocol_version: PROTOCOL_VERSION,
            batch: AggregateBatch {
                batch_id: batch,
                source_instance_id: source,
                sequence,
                period: ObservationPeriod {
                    start: Utc.with_ymd_and_hms(2026, 6, 14, 12, 0, 0).unwrap(),
                    end: Utc.with_ymd_and_hms(2026, 6, 14, 12, 15, 0).unwrap(),
                    time_zone: "America/Sao_Paulo".to_owned(),
                },
                metrics: vec![Metric {
                    signal: SignalId::BrowserTabSwitchCount,
                    value: 18.0,
                }],
            },
        }
    }

    fn request_with_scrolling(
        source: Uuid,
        batch_id: Uuid,
        sequence: u64,
        seconds: f64,
    ) -> ProtocolRequest {
        let mut request = request(source, batch_id, sequence);
        let ProtocolRequest::IngestAggregate { batch, .. } = &mut request else {
            unreachable!();
        };
        batch.metrics.push(Metric {
            signal: SignalId::BrowserContinuousScrollingSeconds,
            value: seconds,
        });
        request
    }

    fn read_export_file(directory: &Path, name: &str) -> String {
        std::fs::read_to_string(directory.join(name)).expect("export file should be written")
    }

    fn check_in_request(check_in_id: Uuid) -> ProtocolRequest {
        ProtocolRequest::SubmitCheckIn {
            protocol_version: PROTOCOL_VERSION,
            check_in: CheckInRecord {
                check_in_id,
                occurred_at: Utc.with_ymd_and_hms(2026, 6, 14, 12, 0, 0).unwrap(),
                time_zone: "America/Sao_Paulo".to_owned(),
                local_date: "2026-06-14".to_owned(),
                sleep_minutes: Some(420),
                perceived_sleep_need: Some(4),
                mood: Some(5),
                energy: Some(6),
                irritability: Some(2),
                concentration: Some(4),
                impulsivity: Some(3),
                medication_taken: Some(true),
                substance_use: Some(false),
                context_tags: vec![ContextTag::Deadline],
            },
        }
    }

    fn check_in_request_at(check_in_id: Uuid, hour: u32, minute: u32) -> ProtocolRequest {
        let mut request = check_in_request(check_in_id);
        let ProtocolRequest::SubmitCheckIn { check_in, .. } = &mut request else {
            unreachable!();
        };
        check_in.occurred_at = Utc.with_ymd_and_hms(2026, 6, 14, hour, minute, 0).unwrap();
        request
    }

    fn annotation_request(annotation_id: Uuid) -> ProtocolRequest {
        ProtocolRequest::SaveAnnotation {
            protocol_version: PROTOCOL_VERSION,
            annotation: AnnotationRecord {
                annotation_id,
                created_at: Utc.with_ymd_and_hms(2026, 6, 14, 12, 10, 0).unwrap(),
                time_zone: "America/Sao_Paulo".to_owned(),
                local_date: "2026-06-14".to_owned(),
                start_minute: Some(8 * 60),
                end_minute: Some(9 * 60),
                text: "Power outage before breakfast".to_owned(),
                context_tags: vec![ContextTag::Other],
            },
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap()
    }

    #[test]
    fn health_reports_the_supported_protocol() {
        let temp = tempfile::TempDir::new().unwrap();
        let response = state(&temp).handle_request(
            ProtocolRequest::Health {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );

        assert!(matches!(
            response,
            ProtocolResponse::Health {
                protocol_version: PROTOCOL_VERSION,
                status: ServiceStatus::Ready,
                ..
            }
        ));
    }

    #[test]
    fn source_status_moves_from_never_seen_to_active_to_stale() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = state(&temp);
        let enabled_at = now() - Duration::minutes(30);
        let _ = state.handle_request(
            ProtocolRequest::SetSignalCollection {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::BrowserTabSwitchCount,
                enabled: true,
            },
            enabled_at,
        );

        let initial = state.handle_request(
            ProtocolRequest::GetSourceStatus {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        assert_source_health(&initial, SourceType::Browser, SourceHealth::NeverSeen, None);

        let received_at = now();
        let ingested =
            state.handle_request(request(Uuid::now_v7(), Uuid::now_v7(), 1), received_at);
        assert!(matches!(
            ingested,
            ProtocolResponse::IngestAcknowledged {
                disposition: IngestDisposition::Stored,
                ..
            }
        ));

        let active = state.handle_request(
            ProtocolRequest::GetSourceStatus {
                protocol_version: PROTOCOL_VERSION,
            },
            received_at + Duration::minutes(45),
        );
        assert_source_health(
            &active,
            SourceType::Browser,
            SourceHealth::Active,
            Some(received_at),
        );

        let stale_response = state.handle_request(
            ProtocolRequest::GetSourceStatus {
                protocol_version: PROTOCOL_VERSION,
            },
            received_at + Duration::minutes(46),
        );
        assert_source_health(
            &stale_response,
            SourceType::Browser,
            SourceHealth::Stale,
            Some(received_at),
        );
    }

    #[test]
    fn source_status_distinguishes_disabled_unavailable_and_ready_sources() {
        let temp = tempfile::TempDir::new().unwrap();
        let response = state(&temp).handle_request(
            ProtocolRequest::GetSourceStatus {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );

        assert_source_health(&response, SourceType::Browser, SourceHealth::Disabled, None);
        assert_source_health(&response, SourceType::Os, SourceHealth::Unavailable, None);
        assert_source_health(&response, SourceType::CheckIn, SourceHealth::Active, None);
    }

    fn assert_source_health(
        response: &ProtocolResponse,
        source: SourceType,
        health: SourceHealth,
        last_received_at: Option<DateTime<Utc>>,
    ) {
        let ProtocolResponse::SourceStatus { sources, .. } = response else {
            panic!("expected source status");
        };
        let status = sources
            .iter()
            .find(|status| status.source == source)
            .expect("source status should be present");
        assert_eq!(status.health, health);
        assert_eq!(status.last_received_at, last_received_at);
    }

    #[test]
    fn reports_platform_capabilities_without_claiming_os_collection() {
        let temp = tempfile::TempDir::new().unwrap();
        let response = state(&temp).handle_request(
            ProtocolRequest::GetPlatformCapabilities {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );

        let ProtocolResponse::PlatformCapabilities { capabilities, .. } = response else {
            panic!("expected platform capabilities");
        };
        assert!(!capabilities.capabilities.is_empty());
        assert!(
            capabilities
                .capabilities
                .iter()
                .all(|capability| { capability.status != PlatformCapabilityStatus::Available })
        );
    }

    #[test]
    fn classifies_linux_gnome_x11_as_planned_before_adapter_start() {
        let capabilities = platform_capabilities_from(
            PlatformEnvironment {
                operating_system: OperatingSystem::Linux,
                desktop_environment: DesktopEnvironment::Gnome,
                session_type: SessionType::X11,
            },
            OsAdapterRuntimeStatus::NotStarted,
        );

        assert!(
            capabilities
                .capabilities
                .iter()
                .all(|capability| { capability.status == PlatformCapabilityStatus::Planned })
        );
        assert!(capabilities.capabilities.iter().all(|capability| {
            capability.detail.contains("has not started")
                || capability.detail.contains("have not connected")
                || capability.detail.contains("separate opt-in adapter")
        }));
    }

    #[test]
    fn reports_lifecycle_available_only_when_both_local_event_streams_connect() {
        let capabilities = platform_capabilities_with_status(
            PlatformEnvironment {
                operating_system: OperatingSystem::Linux,
                desktop_environment: DesktopEnvironment::Gnome,
                session_type: SessionType::X11,
            },
            OsAdapterRuntimeStatus::Available,
            OsLifecycleRuntimeStatus {
                lock_events: true,
                sleep_events: true,
            },
        );
        let lifecycle = capabilities
            .capabilities
            .iter()
            .find(|capability| {
                capability.capability == PlatformCapabilityId::OsLockAndSessionEvents
            })
            .unwrap();
        assert_eq!(lifecycle.status, PlatformCapabilityStatus::Available);
        assert!(lifecycle.detail.contains("explicitly enabled"));
    }

    #[test]
    fn reports_active_idle_available_when_gnome_x11_adapter_connects() {
        let capabilities = platform_capabilities_from(
            PlatformEnvironment {
                operating_system: OperatingSystem::Linux,
                desktop_environment: DesktopEnvironment::Gnome,
                session_type: SessionType::X11,
            },
            OsAdapterRuntimeStatus::Available,
        );

        let active_idle = capabilities
            .capabilities
            .iter()
            .find(|capability| capability.capability == PlatformCapabilityId::OsActiveIdleDuration)
            .expect("active-idle capability should be present");
        assert_eq!(active_idle.status, PlatformCapabilityStatus::Available);
        assert!(active_idle.detail.contains("collection remains off"));

        assert!(capabilities.capabilities.iter().any(|capability| {
            capability.capability == PlatformCapabilityId::OsLockAndSessionEvents
                && capability.status == PlatformCapabilityStatus::Planned
        }));
    }

    #[test]
    fn classifies_wayland_as_unavailable_until_a_separate_adapter_exists() {
        let capabilities = platform_capabilities_from(
            PlatformEnvironment {
                operating_system: OperatingSystem::Linux,
                desktop_environment: DesktopEnvironment::Gnome,
                session_type: SessionType::Wayland,
            },
            OsAdapterRuntimeStatus::Available,
        );

        assert!(
            capabilities
                .capabilities
                .iter()
                .all(|capability| { capability.status == PlatformCapabilityStatus::Unavailable })
        );
    }

    #[test]
    fn collection_is_disabled_until_each_signal_is_enabled() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = state(&temp);
        let response = state.handle_request(
            ProtocolRequest::GetCollectionSettings {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        let ProtocolResponse::CollectionSettings { settings, .. } = response else {
            panic!("expected collection settings");
        };
        assert_eq!(settings.len(), SignalId::ALL.len());
        assert!(settings.iter().all(|setting| !setting.enabled));

        let discarded = state.handle_request(request(Uuid::now_v7(), Uuid::now_v7(), 1), now());
        assert!(matches!(
            discarded,
            ProtocolResponse::IngestAcknowledged {
                disposition: IngestDisposition::DiscardedDisabled,
                ..
            }
        ));

        let store = state.store.lock().unwrap();
        assert_eq!(store.aggregate_batch_count().unwrap(), 0);
        assert_eq!(store.metric_count().unwrap(), 0);
    }

    #[test]
    fn mixed_batches_store_only_continuously_enabled_signals() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = state(&temp);
        let enabled_at = Utc.with_ymd_and_hms(2026, 6, 14, 11, 55, 0).unwrap();
        let response = state.handle_request(
            ProtocolRequest::SetSignalCollection {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::BrowserTabSwitchCount,
                enabled: true,
            },
            enabled_at,
        );
        assert!(matches!(
            response,
            ProtocolResponse::CollectionSettings { .. }
        ));

        let mut mixed = request(Uuid::now_v7(), Uuid::now_v7(), 1);
        let ProtocolRequest::IngestAggregate { batch, .. } = &mut mixed else {
            unreachable!();
        };
        batch.metrics.push(Metric {
            signal: SignalId::BrowserOpenTabCountMax,
            value: 31.0,
        });

        let response = state.handle_request(mixed, now());
        assert!(matches!(
            response,
            ProtocolResponse::IngestAcknowledged {
                disposition: IngestDisposition::StoredFiltered,
                ..
            }
        ));

        let store = state.store.lock().unwrap();
        assert_eq!(store.aggregate_batch_count().unwrap(), 1);
        assert_eq!(store.metric_count().unwrap(), 1);
    }

    #[test]
    fn delayed_retries_cannot_backfill_a_paused_period() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = state(&temp);
        let signal = SignalId::BrowserTabSwitchCount;

        let _ = state.handle_request(
            ProtocolRequest::SetSignalCollection {
                protocol_version: PROTOCOL_VERSION,
                signal,
                enabled: true,
            },
            Utc.with_ymd_and_hms(2026, 6, 14, 11, 55, 0).unwrap(),
        );
        let _ = state.handle_request(
            ProtocolRequest::SetSignalCollection {
                protocol_version: PROTOCOL_VERSION,
                signal,
                enabled: false,
            },
            Utc.with_ymd_and_hms(2026, 6, 14, 12, 5, 0).unwrap(),
        );
        let _ = state.handle_request(
            ProtocolRequest::SetSignalCollection {
                protocol_version: PROTOCOL_VERSION,
                signal,
                enabled: true,
            },
            now(),
        );

        let response = state.handle_request(request(Uuid::now_v7(), Uuid::now_v7(), 1), now());
        assert!(matches!(
            response,
            ProtocolResponse::IngestAcknowledged {
                disposition: IngestDisposition::DiscardedDisabled,
                ..
            }
        ));
        assert_eq!(
            state.store.lock().unwrap().aggregate_batch_count().unwrap(),
            0
        );
    }

    #[test]
    fn duplicate_batches_are_idempotent() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = enabled_state(&temp);
        let source = Uuid::now_v7();
        let batch = Uuid::now_v7();

        let first = state.handle_request(request(source, batch, 1), now());
        let second = state.handle_request(request(source, batch, 1), now());

        assert!(matches!(
            first,
            ProtocolResponse::IngestAcknowledged {
                disposition: IngestDisposition::Stored,
                ..
            }
        ));
        assert!(matches!(
            second,
            ProtocolResponse::IngestAcknowledged {
                disposition: IngestDisposition::Duplicate,
                ..
            }
        ));
    }

    #[test]
    fn duplicate_check_ins_are_idempotent() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = state(&temp);
        let check_in_id = Uuid::now_v7();

        let first = state.handle_request(check_in_request(check_in_id), now());
        let second = state.handle_request(check_in_request(check_in_id), now());

        assert!(matches!(
            first,
            ProtocolResponse::CheckInAcknowledged {
                disposition: IngestDisposition::Stored,
                ..
            }
        ));
        assert!(matches!(
            second,
            ProtocolResponse::CheckInAcknowledged {
                disposition: IngestDisposition::Duplicate,
                ..
            }
        ));
    }

    #[test]
    fn confirmation_bound_delete_removes_latest_check_in_for_day() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = state(&temp);
        let first_id = Uuid::now_v7();
        let second_id = Uuid::now_v7();

        let _ = state.handle_request(check_in_request_at(first_id, 12, 0), now());
        let _ = state.handle_request(check_in_request_at(second_id, 12, 5), now());

        let prepared = state.handle_request(
            ProtocolRequest::PrepareDeleteLatestCheckIn {
                protocol_version: PROTOCOL_VERSION,
                local_date: "2026-06-14".to_owned(),
            },
            now(),
        );
        let ProtocolResponse::DeleteLatestCheckInConfirmation {
            confirmation_token,
            check_in_id,
            ..
        } = prepared
        else {
            panic!("expected latest check-in delete confirmation");
        };
        assert_eq!(check_in_id, second_id);

        assert_eq!(
            state.handle_request(
                ProtocolRequest::DeleteLatestCheckIn {
                    protocol_version: PROTOCOL_VERSION,
                    local_date: "2026-06-14".to_owned(),
                    confirmation_token,
                },
                now(),
            ),
            ProtocolResponse::CheckInDeleted {
                protocol_version: PROTOCOL_VERSION,
                local_date: "2026-06-14".to_owned(),
                check_in_id: second_id,
            }
        );

        let store = state.store.lock().unwrap();
        assert_eq!(store.check_in_count().unwrap(), 1);
        assert_eq!(
            store
                .latest_check_in_id_for_local_date("2026-06-14")
                .unwrap(),
            Some(first_id)
        );
    }

    #[test]
    fn annotation_save_timeline_and_confirmation_bound_delete_round_trip() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = state(&temp);
        let annotation_id = Uuid::now_v7();

        assert_eq!(
            state.handle_request(annotation_request(annotation_id), now()),
            ProtocolResponse::AnnotationSaved {
                protocol_version: PROTOCOL_VERSION,
                annotation_id,
            }
        );

        let timeline = state.handle_request(
            ProtocolRequest::GetDailyTimeline {
                protocol_version: PROTOCOL_VERSION,
                limit: None,
            },
            now(),
        );
        let ProtocolResponse::DailyTimeline { summary, days, .. } = timeline else {
            panic!("expected timeline");
        };
        assert_eq!(summary.annotation_day_count, 1);
        assert_eq!(days[0].annotations[0].text, "Power outage before breakfast");

        let prepared = state.handle_request(
            ProtocolRequest::PrepareDeleteAnnotation {
                protocol_version: PROTOCOL_VERSION,
                annotation_id,
            },
            now(),
        );
        let ProtocolResponse::DeleteAnnotationConfirmation {
            confirmation_token, ..
        } = prepared
        else {
            panic!("expected annotation delete confirmation");
        };
        assert_eq!(
            state.handle_request(
                ProtocolRequest::DeleteAnnotation {
                    protocol_version: PROTOCOL_VERSION,
                    annotation_id,
                    confirmation_token,
                },
                now(),
            ),
            ProtocolResponse::AnnotationDeleted {
                protocol_version: PROTOCOL_VERSION,
                annotation_id,
            }
        );

        let timeline = state.handle_request(
            ProtocolRequest::GetDailyTimeline {
                protocol_version: PROTOCOL_VERSION,
                limit: None,
            },
            now(),
        );
        assert!(matches!(
            timeline,
            ProtocolResponse::DailyTimeline { ref days, .. } if days.is_empty()
        ));
    }

    #[test]
    fn stale_source_sequences_are_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = enabled_state(&temp);
        let source = Uuid::now_v7();

        let _ = state.handle_request(request(source, Uuid::now_v7(), 2), now());
        let response = state.handle_request(request(source, Uuid::now_v7(), 1), now());

        assert_eq!(
            response,
            ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::SequenceConflict
            }
        );
    }

    #[test]
    fn returns_daily_rhythm_insights_from_local_store() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = enabled_state(&temp);

        let mut requests = mindcanary_test_support::synthetic_browser_requests();
        requests.extend(mindcanary_test_support::synthetic_check_in_requests());
        for request in requests {
            let response = state.handle_request(request, now());
            assert!(matches!(
                response,
                ProtocolResponse::IngestAcknowledged {
                    disposition: IngestDisposition::Stored,
                    ..
                } | ProtocolResponse::CheckInAcknowledged {
                    disposition: IngestDisposition::Stored,
                    ..
                }
            ));
        }

        let response = state.handle_request(
            ProtocolRequest::GetDailyRhythmInsights {
                protocol_version: PROTOCOL_VERSION,
                limit: Some(MAX_DAILY_RHYTHM_INSIGHT_LIMIT),
            },
            now(),
        );

        let ProtocolResponse::DailyRhythmInsights {
            summary,
            readiness,
            insights,
            ..
        } = response
        else {
            panic!("expected daily rhythm insights response");
        };

        assert_eq!(summary.daily_snapshot_count, 5);
        assert_eq!(summary.browser_day_count, 5);
        assert_eq!(summary.check_in_day_count, 5);
        assert!(summary.insight_count_before_limit >= insights.len());
        assert!(insights.len() <= usize::from(MAX_DAILY_RHYTHM_INSIGHT_LIMIT));
        assert!(
            readiness
                .iter()
                .any(|item| item.status == RhythmReadinessStatus::ChangeDescribed)
        );
        assert!(
            insights
                .iter()
                .any(|insight| insight.dimension == RhythmInsightDimension::BrowserTabs)
        );
        assert_neutral_language(&insights);

        let first_current = insights
            .first()
            .expect("synthetic history should emit insights");
        let limited = state.handle_request(
            ProtocolRequest::GetDailyRhythmInsights {
                protocol_version: PROTOCOL_VERSION,
                limit: Some(1),
            },
            now(),
        );
        let ProtocolResponse::DailyRhythmInsights {
            insights: limited, ..
        } = limited
        else {
            panic!("expected limited daily rhythm insights response");
        };
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].local_date, first_current.local_date);
        assert_eq!(limited[0].dimension, first_current.dimension);
    }

    #[test]
    fn returns_a_typed_daily_timeline_with_explicit_gaps() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = enabled_state(&temp);
        let mut browser_request = request_with_scrolling(Uuid::now_v7(), Uuid::now_v7(), 1, 180.0);
        let ProtocolRequest::IngestAggregate { batch, .. } = &mut browser_request else {
            unreachable!();
        };
        batch.period.start = Utc.with_ymd_and_hms(2026, 6, 12, 12, 0, 0).unwrap();
        batch.period.end = Utc.with_ymd_and_hms(2026, 6, 12, 12, 15, 0).unwrap();

        let _ = state.handle_request(browser_request, now());
        let _ = state.handle_request(check_in_request(Uuid::now_v7()), now());
        let response = state.handle_request(
            ProtocolRequest::GetDailyTimeline {
                protocol_version: PROTOCOL_VERSION,
                limit: Some(3),
            },
            now(),
        );

        let ProtocolResponse::DailyTimeline { summary, days, .. } = response else {
            panic!("expected daily timeline response");
        };
        assert_eq!(summary.calendar_day_count_before_limit, 3);
        assert_eq!(summary.returned_day_count, 3);
        assert_eq!(summary.browser_day_count, 1);
        assert_eq!(summary.check_in_day_count, 1);
        assert_eq!(summary.missing_day_count, 1);
        assert!(!summary.days_truncated);
        assert_eq!(
            days.iter()
                .map(|day| day.local_date.as_str())
                .collect::<Vec<_>>(),
            ["2026-06-12", "2026-06-13", "2026-06-14"]
        );
        assert_eq!(
            days[0]
                .browser
                .as_ref()
                .and_then(|browser| browser.continuous_scrolling_seconds),
            Some(180.0)
        );
        assert!(days[1].browser.is_none() && days[1].check_in.is_none());
        assert_eq!(
            days[2].check_in.as_ref().unwrap().context_tags,
            [ContextTag::Deadline]
        );

        let limited = state.handle_request(
            ProtocolRequest::GetDailyTimeline {
                protocol_version: PROTOCOL_VERSION,
                limit: Some(2),
            },
            now(),
        );
        let ProtocolResponse::DailyTimeline { summary, days, .. } = limited else {
            panic!("expected limited daily timeline response");
        };
        assert!(summary.days_truncated);
        assert_eq!(summary.returned_day_count, 2);
        assert_eq!(days[0].local_date, "2026-06-13");
    }

    #[test]
    fn local_export_requires_confirmation_and_writes_report_and_csvs() {
        let temp = tempfile::TempDir::new().unwrap();
        let export_dir = tempfile::TempDir::new().unwrap();
        let state = enabled_state(&temp);
        let source = Uuid::now_v7();

        let browser_request = request_with_scrolling(source, Uuid::now_v7(), 1, 180.0);
        let _ = state.handle_request(browser_request, now());
        let _ = state.handle_request(check_in_request(Uuid::now_v7()), now());
        let annotation_id = Uuid::now_v7();
        let _ = state.handle_request(annotation_request(annotation_id), now());

        let prepared = state.handle_request(
            ProtocolRequest::PrepareExportLocalRecords {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        let ProtocolResponse::ExportLocalRecordsConfirmation {
            confirmation_token,
            summary,
            ..
        } = prepared
        else {
            panic!("expected export confirmation");
        };
        assert_eq!(summary.aggregate_batch_count, 1);
        assert_eq!(summary.check_in_count, 1);
        assert_eq!(summary.annotation_count, 1);

        let wrong_token = state.handle_request(
            ProtocolRequest::ExportLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token: Uuid::now_v7(),
                export_directory: export_dir.path().display().to_string(),
            },
            now(),
        );
        assert_eq!(
            wrong_token,
            ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidConfirmation
            }
        );

        let exported = state.handle_request(
            ProtocolRequest::ExportLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token,
                export_directory: export_dir.path().display().to_string(),
            },
            now(),
        );
        let ProtocolResponse::LocalRecordsExported { export, .. } = exported else {
            panic!("expected local records exported response");
        };
        assert_eq!(export.summary.aggregate_batch_count, 1);
        assert_eq!(export.summary.check_in_count, 1);

        let report = read_export_file(export_dir.path(), "mindcanary-report.md");
        assert!(report.contains("MindCanary Local Export"));
        assert!(report.contains("does not contain URLs, page titles, page text, keystrokes"));
        assert!(report.contains("Annotations: 1"));

        let browser_csv = read_export_file(export_dir.path(), "daily-browser.csv");
        assert!(browser_csv.contains("continuous_scrolling_seconds"));
        assert!(browser_csv.contains("2026-06-14,,,18.000,,180.000,,,1"));

        let check_in_csv = read_export_file(export_dir.path(), "daily-check-ins.csv");
        assert!(check_in_csv.contains("local_date,sleep_minutes,mood,energy"));
        assert!(check_in_csv.contains("deadline"));

        let annotation_csv = read_export_file(export_dir.path(), "annotations.csv");
        assert!(annotation_csv.contains("Power outage before breakfast"));
        assert!(annotation_csv.contains(&annotation_id.to_string()));

        let summary_after_export = state.handle_request(
            ProtocolRequest::GetLocalDataSummary {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        assert!(matches!(
            summary_after_export,
            ProtocolResponse::LocalDataSummary {
                summary: LocalDataSummary {
                    aggregate_batch_count: 1,
                    check_in_count: 1,
                    annotation_count: 1,
                    ..
                },
                ..
            }
        ));

        let replay = state.handle_request(
            ProtocolRequest::ExportLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token,
                export_directory: export_dir.path().display().to_string(),
            },
            now(),
        );
        assert_eq!(
            replay,
            ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidConfirmation
            }
        );
    }

    #[test]
    fn local_export_rejects_relative_directories() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = state(&temp);
        let prepared = state.handle_request(
            ProtocolRequest::PrepareExportLocalRecords {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        let ProtocolResponse::ExportLocalRecordsConfirmation {
            confirmation_token, ..
        } = prepared
        else {
            panic!("expected export confirmation");
        };

        let response = state.handle_request(
            ProtocolRequest::ExportLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token,
                export_directory: "relative-export".to_owned(),
            },
            now(),
        );

        assert_eq!(
            response,
            ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidRequest
            }
        );
    }

    #[test]
    fn encrypted_backup_create_verify_and_empty_profile_restore_round_trip() {
        let temp = tempfile::TempDir::new().unwrap();
        let backup_dir = tempfile::TempDir::new().unwrap();
        let backup_path = backup_dir.path().join("history.mcbak");
        let state = enabled_state(&temp);

        let _ = state.handle_request(request(Uuid::now_v7(), Uuid::now_v7(), 1), now());
        let _ = state.handle_request(check_in_request(Uuid::now_v7()), now());
        let _ = state.handle_request(annotation_request(Uuid::now_v7()), now());

        let prepared = state.handle_request(
            ProtocolRequest::PrepareCreateLocalBackup {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        let ProtocolResponse::CreateLocalBackupConfirmation {
            confirmation_token,
            summary,
            ..
        } = prepared
        else {
            panic!("expected backup confirmation");
        };
        assert_eq!(summary.aggregate_batch_count, 1);
        assert_eq!(summary.check_in_count, 1);
        assert_eq!(summary.annotation_count, 1);

        let created = state.handle_request(
            ProtocolRequest::CreateLocalBackup {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token,
                backup_path: backup_path.display().to_string(),
            },
            now(),
        );
        let ProtocolResponse::LocalBackupCreated { backup, .. } = created else {
            panic!("expected created backup");
        };
        assert!(backup_path.is_file());
        assert_eq!(backup.summary, summary);
        assert_eq!(backup.recovery_secret.replace('-', "").len(), 64);
        let encrypted_bytes = std::fs::read(&backup_path).unwrap();
        assert!(
            !encrypted_bytes
                .windows("Power outage before breakfast".len())
                .any(|window| window == b"Power outage before breakfast")
        );

        assert!(matches!(
            state.handle_request(
                ProtocolRequest::VerifyLocalBackup {
                    protocol_version: PROTOCOL_VERSION,
                    backup_path: backup_path.display().to_string(),
                    recovery_secret: backup.recovery_secret.clone(),
                },
                now(),
            ),
            ProtocolResponse::LocalBackupVerified { .. }
        ));

        let clear = state.handle_request(
            ProtocolRequest::PrepareClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        let ProtocolResponse::ClearLocalRecordsConfirmation {
            confirmation_token, ..
        } = clear
        else {
            panic!("expected clear confirmation");
        };
        let _ = state.handle_request(
            ProtocolRequest::ClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token,
            },
            now(),
        );

        let restored = state.handle_request(
            ProtocolRequest::RestoreLocalBackup {
                protocol_version: PROTOCOL_VERSION,
                backup_path: backup_path.display().to_string(),
                recovery_secret: backup.recovery_secret,
            },
            now(),
        );
        assert!(matches!(
            restored,
            ProtocolResponse::LocalBackupRestored {
                restored: LocalDataSummary {
                    aggregate_batch_count: 1,
                    check_in_count: 1,
                    annotation_count: 1,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn local_record_clear_requires_a_valid_unexpired_confirmation() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = enabled_state(&temp);
        let source = Uuid::now_v7();

        let stored = state.handle_request(request(source, Uuid::now_v7(), 1), now());
        assert!(matches!(
            stored,
            ProtocolResponse::IngestAcknowledged {
                disposition: IngestDisposition::Stored,
                ..
            }
        ));

        let prepared = state.handle_request(
            ProtocolRequest::PrepareClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        let ProtocolResponse::ClearLocalRecordsConfirmation {
            confirmation_token,
            summary,
            ..
        } = prepared
        else {
            panic!("expected clear confirmation");
        };
        assert_eq!(summary.aggregate_batch_count, 1);

        let wrong_token = state.handle_request(
            ProtocolRequest::ClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token: Uuid::now_v7(),
            },
            now(),
        );
        assert_eq!(
            wrong_token,
            ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidConfirmation
            }
        );

        let expired = state.handle_request(
            ProtocolRequest::ClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token,
            },
            now() + chrono::Duration::minutes(CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES + 1),
        );
        assert_eq!(
            expired,
            ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidConfirmation
            }
        );

        let summary = state.handle_request(
            ProtocolRequest::GetLocalDataSummary {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        assert!(matches!(
            summary,
            ProtocolResponse::LocalDataSummary {
                summary: LocalDataSummary {
                    aggregate_batch_count: 1,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn signal_deletion_confirmation_is_bound_and_preserves_other_records() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = enabled_state(&temp);
        let mut mixed = request(Uuid::now_v7(), Uuid::now_v7(), 1);
        let ProtocolRequest::IngestAggregate { batch, .. } = &mut mixed else {
            unreachable!();
        };
        batch.metrics.push(Metric {
            signal: SignalId::BrowserOpenTabCountMax,
            value: 31.0,
        });
        let _ = state.handle_request(mixed, now());
        let _ = state.handle_request(check_in_request(Uuid::now_v7()), now());

        let prepared = state.handle_request(
            ProtocolRequest::PrepareDeleteSignalRecords {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::BrowserTabSwitchCount,
            },
            now(),
        );
        let ProtocolResponse::DeleteSignalRecordsConfirmation {
            confirmation_token,
            summary,
            ..
        } = prepared
        else {
            panic!("expected signal deletion confirmation");
        };
        assert_eq!(
            summary,
            mindcanary_protocol::SignalRecordSummary {
                metric_record_count: 1,
                affected_batch_count: 1,
            }
        );

        let wrong_signal = state.handle_request(
            ProtocolRequest::DeleteSignalRecords {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::BrowserOpenTabCountMax,
                confirmation_token,
            },
            now(),
        );
        assert_eq!(wrong_signal, invalid_confirmation_response());

        let deleted = state.handle_request(
            ProtocolRequest::DeleteSignalRecords {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::BrowserTabSwitchCount,
                confirmation_token,
            },
            now(),
        );
        assert!(matches!(
            deleted,
            ProtocolResponse::SignalRecordsDeleted {
                signal: SignalId::BrowserTabSwitchCount,
                ..
            }
        ));

        let store = state.store.lock().unwrap();
        assert_eq!(store.aggregate_batch_count().unwrap(), 1);
        assert_eq!(store.metric_count().unwrap(), 1);
        assert_eq!(store.check_in_count().unwrap(), 1);
        assert_eq!(
            store
                .signal_record_summary(SignalId::BrowserOpenTabCountMax)
                .unwrap()
                .metric_record_count,
            1
        );
    }

    #[test]
    fn valid_confirmation_clears_records_once() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = enabled_state(&temp);
        let source = Uuid::now_v7();
        let _ = state.handle_request(request(source, Uuid::now_v7(), 1), now());
        let _ = state.handle_request(check_in_request(Uuid::now_v7()), now());

        let prepared = state.handle_request(
            ProtocolRequest::PrepareClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        let ProtocolResponse::ClearLocalRecordsConfirmation {
            confirmation_token,
            summary,
            ..
        } = prepared
        else {
            panic!("expected clear confirmation");
        };
        assert_eq!(summary.aggregate_batch_count, 1);
        assert_eq!(summary.check_in_count, 1);

        let cleared = state.handle_request(
            ProtocolRequest::ClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token,
            },
            now(),
        );
        assert!(matches!(
            cleared,
            ProtocolResponse::LocalRecordsCleared {
                deleted: LocalDataSummary {
                    aggregate_batch_count: 1,
                    check_in_count: 1,
                    ..
                },
                ..
            }
        ));

        let reused = state.handle_request(
            ProtocolRequest::ClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token,
            },
            now(),
        );
        assert_eq!(
            reused,
            ProtocolResponse::Error {
                protocol_version: PROTOCOL_VERSION,
                code: ErrorCode::InvalidConfirmation
            }
        );

        let summary = state.handle_request(
            ProtocolRequest::GetLocalDataSummary {
                protocol_version: PROTOCOL_VERSION,
            },
            now(),
        );
        assert!(matches!(
            summary,
            ProtocolResponse::LocalDataSummary {
                summary: LocalDataSummary {
                    aggregate_batch_count: 0,
                    aggregate_metric_count: 0,
                    check_in_count: 0,
                    context_tag_count: 0,
                    ..
                },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn unix_socket_round_trip_uses_private_permissions() {
        let runtime_dir =
            std::env::temp_dir().join(format!("mindcanary-daemon-test-{}", Uuid::now_v7()));
        let socket_path = runtime_dir.join("mindcanaryd.sock");
        let server_path = socket_path.clone();
        let data_dir = tempfile::TempDir::new().unwrap();
        let state = std::sync::Arc::new(state(&data_dir));
        let server = tokio::spawn(async move { run_with_state(&server_path, state).await });

        for _ in 0..100 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(socket_path.exists());

        let socket_mode = std::fs::metadata(&socket_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(socket_mode, 0o600);

        let mut stream = UnixStream::connect(&socket_path).await.unwrap();
        let health = ProtocolRequest::Health {
            protocol_version: PROTOCOL_VERSION,
        };
        write_frame(&mut stream, &serde_json::to_vec(&health).unwrap())
            .await
            .unwrap();
        let response = read_frame(&mut stream).await.unwrap().unwrap();
        let response: ProtocolResponse = serde_json::from_slice(&response).unwrap();

        assert!(matches!(
            response,
            ProtocolResponse::Health {
                status: ServiceStatus::Ready,
                ..
            }
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(runtime_dir);
    }

    #[test]
    fn duplicate_state_survives_store_reopen() {
        let temp = tempfile::TempDir::new().unwrap();
        let key = mindcanary_storage::DatabaseKey::from_bytes([9; 32]);
        let database_path = temp.path().join("mindcanary.db");
        let source = Uuid::now_v7();
        let batch = Uuid::now_v7();

        let mut first_store = EncryptedStore::open(&database_path, &key).unwrap();
        first_store
            .set_signal_collection(
                SignalId::BrowserTabSwitchCount,
                true,
                Utc.with_ymd_and_hms(2026, 6, 14, 11, 55, 0).unwrap(),
            )
            .unwrap();
        let first_state = DaemonState::new(first_store);
        let first = first_state.handle_request(request(source, batch, 1), now());
        assert!(matches!(
            first,
            ProtocolResponse::IngestAcknowledged {
                disposition: mindcanary_protocol::IngestDisposition::Stored,
                ..
            }
        ));
        drop(first_state);

        let second_state = DaemonState::new(EncryptedStore::open(&database_path, &key).unwrap());
        let second = second_state.handle_request(request(source, batch, 1), now());
        assert!(matches!(
            second,
            ProtocolResponse::IngestAcknowledged {
                disposition: mindcanary_protocol::IngestDisposition::Duplicate,
                ..
            }
        ));
    }

    #[test]
    fn check_in_state_survives_store_reopen() {
        let temp = tempfile::TempDir::new().unwrap();
        let key = mindcanary_storage::DatabaseKey::from_bytes([10; 32]);
        let database_path = temp.path().join("mindcanary.db");
        let check_in_id = Uuid::now_v7();

        let first_state = DaemonState::new(EncryptedStore::open(&database_path, &key).unwrap());
        let first = first_state.handle_request(check_in_request(check_in_id), now());
        assert!(matches!(
            first,
            ProtocolResponse::CheckInAcknowledged {
                disposition: IngestDisposition::Stored,
                ..
            }
        ));
        drop(first_state);

        let second_state = DaemonState::new(EncryptedStore::open(&database_path, &key).unwrap());
        let second = second_state.handle_request(check_in_request(check_in_id), now());
        assert!(matches!(
            second,
            ProtocolResponse::CheckInAcknowledged {
                disposition: IngestDisposition::Duplicate,
                ..
            }
        ));
    }

    fn assert_neutral_language(insights: &[RhythmInsight]) {
        const BLOCKED_TERMS: [&str; 8] = [
            "mania",
            "manic",
            "depression",
            "depressive",
            "psychosis",
            "diagnosis",
            "warning",
            "risk",
        ];

        for insight in insights {
            let mut text = insight.summary.to_lowercase();
            for evidence in &insight.evidence {
                text.push(' ');
                text.push_str(&evidence.label.to_lowercase());
                text.push(' ');
                text.push_str(&evidence.value.to_lowercase());
            }

            for blocked in BLOCKED_TERMS {
                assert!(
                    !text.contains(blocked),
                    "insight used blocked term {blocked:?}: {text}"
                );
            }
        }
    }
}
