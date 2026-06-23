use std::fmt::Write as _;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_METRICS_PER_BATCH: usize = 64;
pub const MAX_PERIOD_MINUTES: i64 = 24 * 60;
pub const MAX_CLOCK_SKEW_MINUTES: i64 = 10;
pub const MAX_SAFE_SEQUENCE: u64 = 9_007_199_254_740_991;
pub const MIN_CHECK_IN_SCALE: u8 = 1;
pub const MAX_CHECK_IN_SCALE: u8 = 7;
pub const MAX_SLEEP_MINUTES: u16 = 24 * 60;
pub const MAX_CONTEXT_TAGS: usize = 8;
pub const MAX_ANNOTATION_TEXT_BYTES: usize = 1_000;
pub const DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT: u16 = 20;
pub const MAX_DAILY_RHYTHM_INSIGHT_LIMIT: u16 = 100;
pub const DEFAULT_DAILY_TIMELINE_LIMIT: u16 = 30;
pub const MAX_DAILY_TIMELINE_LIMIT: u16 = 366;
pub const CLEAR_LOCAL_RECORDS_CONFIRMATION_MINUTES: i64 = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProtocolRequest {
    Health {
        protocol_version: u16,
    },
    GetSourceStatus {
        protocol_version: u16,
    },
    IngestAggregate {
        protocol_version: u16,
        batch: AggregateBatch,
    },
    SubmitCheckIn {
        protocol_version: u16,
        check_in: CheckInRecord,
    },
    PrepareDeleteLatestCheckIn {
        protocol_version: u16,
        local_date: String,
    },
    DeleteLatestCheckIn {
        protocol_version: u16,
        local_date: String,
        confirmation_token: Uuid,
    },
    SaveAnnotation {
        protocol_version: u16,
        annotation: AnnotationRecord,
    },
    PrepareDeleteAnnotation {
        protocol_version: u16,
        annotation_id: Uuid,
    },
    DeleteAnnotation {
        protocol_version: u16,
        annotation_id: Uuid,
        confirmation_token: Uuid,
    },
    GetDailyRhythmInsights {
        protocol_version: u16,
        limit: Option<u16>,
    },
    GetDailyTimeline {
        protocol_version: u16,
        limit: Option<u16>,
    },
    GetCollectionSettings {
        protocol_version: u16,
    },
    GetPlatformCapabilities {
        protocol_version: u16,
    },
    SetSignalCollection {
        protocol_version: u16,
        signal: SignalId,
        enabled: bool,
    },
    PrepareDeleteSignalRecords {
        protocol_version: u16,
        signal: SignalId,
    },
    DeleteSignalRecords {
        protocol_version: u16,
        signal: SignalId,
        confirmation_token: Uuid,
    },
    GetLocalDataSummary {
        protocol_version: u16,
    },
    PrepareExportLocalRecords {
        protocol_version: u16,
    },
    ExportLocalRecords {
        protocol_version: u16,
        confirmation_token: Uuid,
        export_directory: String,
    },
    PrepareCreateLocalBackup {
        protocol_version: u16,
    },
    CreateLocalBackup {
        protocol_version: u16,
        confirmation_token: Uuid,
        backup_path: String,
    },
    VerifyLocalBackup {
        protocol_version: u16,
        backup_path: String,
        recovery_secret: String,
    },
    RestoreLocalBackup {
        protocol_version: u16,
        backup_path: String,
        recovery_secret: String,
    },
    PrepareClearLocalRecords {
        protocol_version: u16,
    },
    ClearLocalRecords {
        protocol_version: u16,
        confirmation_token: Uuid,
    },
}

impl ProtocolRequest {
    pub fn protocol_version(&self) -> u16 {
        match self {
            Self::Health { protocol_version }
            | Self::GetSourceStatus { protocol_version }
            | Self::IngestAggregate {
                protocol_version, ..
            }
            | Self::SubmitCheckIn {
                protocol_version, ..
            }
            | Self::PrepareDeleteLatestCheckIn {
                protocol_version, ..
            }
            | Self::DeleteLatestCheckIn {
                protocol_version, ..
            }
            | Self::SaveAnnotation {
                protocol_version, ..
            }
            | Self::PrepareDeleteAnnotation {
                protocol_version, ..
            }
            | Self::DeleteAnnotation {
                protocol_version, ..
            }
            | Self::GetDailyRhythmInsights {
                protocol_version, ..
            }
            | Self::GetDailyTimeline {
                protocol_version, ..
            }
            | Self::GetCollectionSettings { protocol_version }
            | Self::GetPlatformCapabilities { protocol_version }
            | Self::SetSignalCollection {
                protocol_version, ..
            }
            | Self::PrepareDeleteSignalRecords {
                protocol_version, ..
            }
            | Self::DeleteSignalRecords {
                protocol_version, ..
            }
            | Self::GetLocalDataSummary { protocol_version }
            | Self::PrepareExportLocalRecords { protocol_version }
            | Self::ExportLocalRecords {
                protocol_version, ..
            }
            | Self::PrepareCreateLocalBackup { protocol_version }
            | Self::CreateLocalBackup {
                protocol_version, ..
            }
            | Self::VerifyLocalBackup {
                protocol_version, ..
            }
            | Self::RestoreLocalBackup {
                protocol_version, ..
            }
            | Self::PrepareClearLocalRecords { protocol_version }
            | Self::ClearLocalRecords {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    pub const fn is_collector_request(&self) -> bool {
        matches!(
            self,
            Self::Health { .. } | Self::IngestAggregate { .. } | Self::GetCollectionSettings { .. }
        )
    }

    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), ValidationError> {
        if self.protocol_version() != PROTOCOL_VERSION {
            return Err(ValidationError::UnsupportedProtocolVersion {
                received: self.protocol_version(),
                supported: PROTOCOL_VERSION,
            });
        }

        match self {
            Self::IngestAggregate { batch, .. } => batch.validate_at(now)?,
            Self::SubmitCheckIn { check_in, .. } => check_in.validate_at(now)?,
            Self::PrepareDeleteLatestCheckIn { local_date, .. }
            | Self::DeleteLatestCheckIn { local_date, .. } => validate_local_date(local_date)?,
            Self::SaveAnnotation { annotation, .. } => annotation.validate_at(now)?,
            Self::GetDailyRhythmInsights { limit, .. } => validate_insight_limit(*limit)?,
            Self::GetDailyTimeline { limit, .. } => validate_timeline_limit(*limit)?,
            Self::ExportLocalRecords {
                export_directory, ..
            } => validate_export_directory(export_directory)?,
            Self::CreateLocalBackup { backup_path, .. } => validate_backup_path(backup_path)?,
            Self::VerifyLocalBackup {
                backup_path,
                recovery_secret,
                ..
            }
            | Self::RestoreLocalBackup {
                backup_path,
                recovery_secret,
                ..
            } => {
                validate_backup_path(backup_path)?;
                validate_recovery_secret(recovery_secret)?;
            }
            Self::Health { .. }
            | Self::GetSourceStatus { .. }
            | Self::GetCollectionSettings { .. }
            | Self::PrepareDeleteAnnotation { .. }
            | Self::DeleteAnnotation { .. }
            | Self::GetPlatformCapabilities { .. }
            | Self::SetSignalCollection { .. }
            | Self::PrepareDeleteSignalRecords { .. }
            | Self::DeleteSignalRecords { .. }
            | Self::GetLocalDataSummary { .. }
            | Self::PrepareExportLocalRecords { .. }
            | Self::PrepareCreateLocalBackup { .. }
            | Self::PrepareClearLocalRecords { .. }
            | Self::ClearLocalRecords { .. } => {}
        }

        Ok(())
    }
}

fn validate_local_date(value: &str) -> Result<(), ValidationError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| ValidationError::InvalidLocalDate)
}

fn validate_backup_path(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err(ValidationError::InvalidBackupPath);
    }
    Ok(())
}

fn validate_recovery_secret(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() || value.len() > 256 || value.contains('\0') {
        return Err(ValidationError::InvalidRecoverySecret);
    }
    Ok(())
}

fn validate_insight_limit(limit: Option<u16>) -> Result<(), ValidationError> {
    if limit.is_some_and(|limit| limit == 0 || limit > MAX_DAILY_RHYTHM_INSIGHT_LIMIT) {
        return Err(ValidationError::InvalidInsightLimit {
            maximum: MAX_DAILY_RHYTHM_INSIGHT_LIMIT,
        });
    }

    Ok(())
}

fn validate_timeline_limit(limit: Option<u16>) -> Result<(), ValidationError> {
    if limit.is_some_and(|limit| limit == 0 || limit > MAX_DAILY_TIMELINE_LIMIT) {
        return Err(ValidationError::InvalidTimelineLimit {
            maximum: MAX_DAILY_TIMELINE_LIMIT,
        });
    }

    Ok(())
}

fn validate_export_directory(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err(ValidationError::InvalidExportDirectory);
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProtocolResponse {
    Health {
        protocol_version: u16,
        service_version: String,
        status: ServiceStatus,
    },
    SourceStatus {
        protocol_version: u16,
        generated_at: DateTime<Utc>,
        sources: Vec<SourceStatus>,
    },
    IngestAcknowledged {
        protocol_version: u16,
        batch_id: Uuid,
        disposition: IngestDisposition,
    },
    CheckInAcknowledged {
        protocol_version: u16,
        check_in_id: Uuid,
        disposition: IngestDisposition,
    },
    DeleteLatestCheckInConfirmation {
        protocol_version: u16,
        confirmation_token: Uuid,
        expires_at: DateTime<Utc>,
        local_date: String,
        check_in_id: Uuid,
    },
    CheckInDeleted {
        protocol_version: u16,
        local_date: String,
        check_in_id: Uuid,
    },
    AnnotationSaved {
        protocol_version: u16,
        annotation_id: Uuid,
    },
    DeleteAnnotationConfirmation {
        protocol_version: u16,
        confirmation_token: Uuid,
        expires_at: DateTime<Utc>,
        annotation_id: Uuid,
    },
    AnnotationDeleted {
        protocol_version: u16,
        annotation_id: Uuid,
    },
    DailyRhythmInsights {
        protocol_version: u16,
        generated_at: DateTime<Utc>,
        summary: DailyRhythmSummary,
        readiness: Vec<RhythmDimensionReadiness>,
        insights: Vec<RhythmInsight>,
    },
    DailyTimeline {
        protocol_version: u16,
        generated_at: DateTime<Utc>,
        summary: DailyTimelineSummary,
        days: Vec<DailyTimelineDay>,
    },
    CollectionSettings {
        protocol_version: u16,
        settings: Vec<SignalCollectionSetting>,
    },
    PlatformCapabilities {
        protocol_version: u16,
        capabilities: PlatformCapabilities,
    },
    DeleteSignalRecordsConfirmation {
        protocol_version: u16,
        confirmation_token: Uuid,
        expires_at: DateTime<Utc>,
        signal: SignalId,
        summary: SignalRecordSummary,
    },
    SignalRecordsDeleted {
        protocol_version: u16,
        signal: SignalId,
        deleted: SignalRecordSummary,
    },
    LocalDataSummary {
        protocol_version: u16,
        summary: LocalDataSummary,
    },
    ExportLocalRecordsConfirmation {
        protocol_version: u16,
        confirmation_token: Uuid,
        expires_at: DateTime<Utc>,
        summary: LocalDataSummary,
    },
    LocalRecordsExported {
        protocol_version: u16,
        export: LocalDataExport,
    },
    CreateLocalBackupConfirmation {
        protocol_version: u16,
        confirmation_token: Uuid,
        expires_at: DateTime<Utc>,
        summary: LocalDataSummary,
    },
    LocalBackupCreated {
        protocol_version: u16,
        backup: LocalBackup,
    },
    LocalBackupVerified {
        protocol_version: u16,
        backup: LocalBackupMetadata,
    },
    LocalBackupRestored {
        protocol_version: u16,
        backup: LocalBackupMetadata,
        restored: LocalDataSummary,
    },
    ClearLocalRecordsConfirmation {
        protocol_version: u16,
        confirmation_token: Uuid,
        expires_at: DateTime<Utc>,
        summary: LocalDataSummary,
    },
    LocalRecordsCleared {
        protocol_version: u16,
        deleted: LocalDataSummary,
    },
    Error {
        protocol_version: u16,
        code: ErrorCode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Ready,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Browser,
    Os,
    CheckIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceHealth {
    NeverSeen,
    Active,
    Stale,
    Disabled,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceStatus {
    pub source: SourceType,
    pub health: SourceHealth,
    pub last_received_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestDisposition {
    Stored,
    StoredFiltered,
    Duplicate,
    DiscardedDisabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidFrame,
    InvalidRequest,
    MessageTooLarge,
    SequenceConflict,
    UnsupportedProtocolVersion,
    InvalidConfirmation,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalDataSummary {
    pub aggregate_batch_count: u64,
    pub aggregate_metric_count: u64,
    pub check_in_count: u64,
    pub context_tag_count: u64,
    #[serde(default)]
    pub annotation_count: u64,
    #[serde(default)]
    pub annotation_context_tag_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalCollectionSetting {
    pub signal: SignalId,
    pub enabled: bool,
    pub changed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalRecordSummary {
    pub metric_record_count: u64,
    pub affected_batch_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalDataExport {
    pub export_directory: String,
    pub report_path: String,
    pub daily_browser_csv_path: String,
    pub daily_os_csv_path: String,
    pub daily_check_in_csv_path: String,
    #[serde(default)]
    pub annotations_csv_path: String,
    pub summary: LocalDataSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalBackup {
    pub backup_path: String,
    pub created_at: DateTime<Utc>,
    pub format_version: i64,
    pub schema_version: i64,
    pub recovery_secret: String,
    pub summary: LocalDataSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalBackupMetadata {
    pub backup_path: String,
    pub created_at: DateTime<Utc>,
    pub format_version: i64,
    pub schema_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformCapabilities {
    pub operating_system: OperatingSystem,
    pub desktop_environment: DesktopEnvironment,
    pub session_type: SessionType,
    pub capabilities: Vec<PlatformCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
    Linux,
    Macos,
    Windows,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
    X11,
    Wayland,
    Other,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformCapability {
    pub capability: PlatformCapabilityId,
    pub status: PlatformCapabilityStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformCapabilityId {
    OsLockAndSessionEvents,
    OsActiveIdleDuration,
    ForegroundApplicationCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformCapabilityStatus {
    Available,
    Planned,
    Unavailable,
}

impl SignalRecordSummary {
    pub const fn is_empty(self) -> bool {
        self.metric_record_count == 0
    }
}

impl LocalDataSummary {
    pub const fn is_empty(self) -> bool {
        self.aggregate_batch_count == 0
            && self.aggregate_metric_count == 0
            && self.check_in_count == 0
            && self.context_tag_count == 0
            && self.annotation_count == 0
            && self.annotation_context_tag_count == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyRhythmSummary {
    pub daily_snapshot_count: usize,
    pub browser_day_count: usize,
    pub os_day_count: usize,
    pub check_in_day_count: usize,
    pub insight_count_before_limit: usize,
    pub insights_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RhythmInsight {
    pub local_date: String,
    pub dimension: RhythmInsightDimension,
    pub direction: RhythmChangeDirection,
    pub summary: String,
    pub evidence: Vec<RhythmEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RhythmDimensionReadiness {
    pub dimension: RhythmInsightDimension,
    pub status: RhythmReadinessStatus,
    pub comparable_day_count: usize,
    pub minimum_day_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RhythmReadinessStatus {
    ChangeDescribed,
    WithinBaseline,
    NeedsSustainedChange,
    MissingCurrent,
    InsufficientBaseline,
    ZeroBaseline,
    UnstableBaseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RhythmInsightDimension {
    BrowserTabs,
    TabSwitching,
    ActiveTime,
    ComputerActiveTime,
    Sleep,
    Energy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RhythmChangeDirection {
    Higher,
    Lower,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RhythmEvidence {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyTimelineSummary {
    pub calendar_day_count_before_limit: usize,
    pub returned_day_count: usize,
    pub browser_day_count: usize,
    pub os_day_count: usize,
    pub check_in_day_count: usize,
    #[serde(default)]
    pub annotation_day_count: usize,
    pub missing_day_count: usize,
    pub days_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyTimelineDay {
    pub local_date: String,
    pub browser: Option<DailyBrowserTimeline>,
    pub os: Option<DailyOsTimeline>,
    pub check_in: Option<DailyCheckInTimeline>,
    #[serde(default)]
    pub annotations: Vec<AnnotationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationRecord {
    pub annotation_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub time_zone: String,
    pub local_date: String,
    pub start_minute: Option<u16>,
    pub end_minute: Option<u16>,
    pub text: String,
    pub context_tags: Vec<ContextTag>,
}

impl AnnotationRecord {
    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), ValidationError> {
        if self.created_at > now + Duration::minutes(MAX_CLOCK_SKEW_MINUTES) {
            return Err(ValidationError::AnnotationInFuture);
        }
        self.time_zone
            .parse::<Tz>()
            .map_err(|_| ValidationError::InvalidTimeZone)?;
        NaiveDate::parse_from_str(&self.local_date, "%Y-%m-%d")
            .map_err(|_| ValidationError::InvalidLocalDate)?;

        if self.text.trim().is_empty() || self.text.len() > MAX_ANNOTATION_TEXT_BYTES {
            return Err(ValidationError::InvalidAnnotationText);
        }
        match (self.start_minute, self.end_minute) {
            (None, None) => {}
            (Some(start), Some(end)) if start < end && end <= 24 * 60 => {}
            _ => return Err(ValidationError::InvalidAnnotationWindow),
        }
        if self.context_tags.len() > MAX_CONTEXT_TAGS {
            return Err(ValidationError::TooManyContextTags {
                count: self.context_tags.len(),
                maximum: MAX_CONTEXT_TAGS,
            });
        }
        for (index, tag) in self.context_tags.iter().enumerate() {
            if self.context_tags[..index].contains(tag) {
                return Err(ValidationError::DuplicateContextTag { tag: *tag });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyBrowserTimeline {
    pub open_tab_count_mean: Option<f64>,
    pub open_tab_count_max: Option<f64>,
    pub tab_switch_count: Option<f64>,
    pub retained_across_day_count: Option<f64>,
    pub continuous_scrolling_seconds: Option<f64>,
    pub active_seconds: Option<f64>,
    pub idle_seconds: Option<f64>,
    pub recorded_bucket_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyOsTimeline {
    pub active_seconds: Option<f64>,
    pub idle_seconds: Option<f64>,
    pub lock_count: Option<f64>,
    pub unlock_count: Option<f64>,
    pub suspend_count: Option<f64>,
    pub resume_count: Option<f64>,
    pub recorded_bucket_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyCheckInTimeline {
    pub sleep_minutes: Option<f64>,
    pub mood: Option<f64>,
    pub energy: Option<f64>,
    pub irritability: Option<f64>,
    pub concentration: Option<f64>,
    pub impulsivity: Option<f64>,
    pub check_in_count: u64,
    pub context_tags: Vec<ContextTag>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckInRecord {
    pub check_in_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub time_zone: String,
    pub local_date: String,
    pub sleep_minutes: Option<u16>,
    pub perceived_sleep_need: Option<u8>,
    pub mood: Option<u8>,
    pub energy: Option<u8>,
    pub irritability: Option<u8>,
    pub concentration: Option<u8>,
    pub impulsivity: Option<u8>,
    pub medication_taken: Option<bool>,
    pub substance_use: Option<bool>,
    pub context_tags: Vec<ContextTag>,
}

impl CheckInRecord {
    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), ValidationError> {
        if self.occurred_at > now + Duration::minutes(MAX_CLOCK_SKEW_MINUTES) {
            return Err(ValidationError::CheckInInFuture);
        }

        let time_zone = self
            .time_zone
            .parse::<Tz>()
            .map_err(|_| ValidationError::InvalidTimeZone)?;
        let local_date = NaiveDate::parse_from_str(&self.local_date, "%Y-%m-%d")
            .map_err(|_| ValidationError::InvalidLocalDate)?;
        if self.occurred_at.with_timezone(&time_zone).date_naive() != local_date {
            return Err(ValidationError::LocalDateMismatch);
        }

        if self
            .sleep_minutes
            .is_some_and(|minutes| minutes > MAX_SLEEP_MINUTES)
        {
            return Err(ValidationError::InvalidSleepMinutes);
        }

        for value in [
            self.perceived_sleep_need,
            self.mood,
            self.energy,
            self.irritability,
            self.concentration,
            self.impulsivity,
        ] {
            validate_scale(value)?;
        }

        if self.context_tags.len() > MAX_CONTEXT_TAGS {
            return Err(ValidationError::TooManyContextTags {
                count: self.context_tags.len(),
                maximum: MAX_CONTEXT_TAGS,
            });
        }

        for (index, tag) in self.context_tags.iter().enumerate() {
            if self.context_tags[..index].contains(tag) {
                return Err(ValidationError::DuplicateContextTag { tag: *tag });
            }
        }

        if self.sleep_minutes.is_none()
            && self.perceived_sleep_need.is_none()
            && self.mood.is_none()
            && self.energy.is_none()
            && self.irritability.is_none()
            && self.concentration.is_none()
            && self.impulsivity.is_none()
            && self.medication_taken.is_none()
            && self.substance_use.is_none()
            && self.context_tags.is_empty()
        {
            return Err(ValidationError::EmptyCheckIn);
        }

        Ok(())
    }
}

fn validate_scale(value: Option<u8>) -> Result<(), ValidationError> {
    if value.is_some_and(|value| !(MIN_CHECK_IN_SCALE..=MAX_CHECK_IN_SCALE).contains(&value)) {
        return Err(ValidationError::InvalidCheckInScale);
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextTag {
    Deadline,
    Travel,
    Illness,
    NewsCycle,
    JobUncertainty,
    SocialConflict,
    Exercise,
    MedicationChange,
    SubstanceUse,
    UnusualGoodEvent,
    Other,
}

impl ContextTag {
    pub const ALL: [Self; 11] = [
        Self::Deadline,
        Self::Travel,
        Self::Illness,
        Self::NewsCycle,
        Self::JobUncertainty,
        Self::SocialConflict,
        Self::Exercise,
        Self::MedicationChange,
        Self::SubstanceUse,
        Self::UnusualGoodEvent,
        Self::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deadline => "deadline",
            Self::Travel => "travel",
            Self::Illness => "illness",
            Self::NewsCycle => "news_cycle",
            Self::JobUncertainty => "job_uncertainty",
            Self::SocialConflict => "social_conflict",
            Self::Exercise => "exercise",
            Self::MedicationChange => "medication_change",
            Self::SubstanceUse => "substance_use",
            Self::UnusualGoodEvent => "unusual_good_event",
            Self::Other => "other",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|tag| tag.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateBatch {
    pub batch_id: Uuid,
    pub source_instance_id: Uuid,
    pub sequence: u64,
    pub period: ObservationPeriod,
    pub metrics: Vec<Metric>,
}

impl AggregateBatch {
    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), ValidationError> {
        self.period.validate_at(now)?;

        if self.sequence > MAX_SAFE_SEQUENCE {
            return Err(ValidationError::SequenceTooLarge);
        }

        if self.metrics.is_empty() {
            return Err(ValidationError::EmptyMetrics);
        }

        if self.metrics.len() > MAX_METRICS_PER_BATCH {
            return Err(ValidationError::TooManyMetrics {
                count: self.metrics.len(),
                maximum: MAX_METRICS_PER_BATCH,
            });
        }

        let period_seconds = (self.period.end - self.period.start)
            .to_std()
            .map_err(|_| ValidationError::InvalidPeriod)?
            .as_secs_f64();

        for metric in &self.metrics {
            metric.validate(period_seconds)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationPeriod {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub time_zone: String,
}

impl ObservationPeriod {
    fn validate_at(&self, now: DateTime<Utc>) -> Result<(), ValidationError> {
        if self.end <= self.start {
            return Err(ValidationError::InvalidPeriod);
        }

        if self.end - self.start > Duration::minutes(MAX_PERIOD_MINUTES) {
            return Err(ValidationError::PeriodTooLong);
        }

        if self.end > now + Duration::minutes(MAX_CLOCK_SKEW_MINUTES) {
            return Err(ValidationError::PeriodInFuture);
        }

        self.time_zone
            .parse::<Tz>()
            .map_err(|_| ValidationError::InvalidTimeZone)?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub signal: SignalId,
    pub value: f64,
}

impl Metric {
    fn validate(&self, period_seconds: f64) -> Result<(), ValidationError> {
        if !self.value.is_finite() {
            return Err(ValidationError::NonFiniteMetric);
        }

        let valid = match self.signal.kind() {
            SignalKind::Count => self.value >= 0.0 && self.value.fract() == 0.0,
            SignalKind::Duration => self.value >= 0.0 && self.value <= period_seconds,
            SignalKind::Mean => self.value >= 0.0,
        };

        if !valid {
            return Err(ValidationError::InvalidMetricValue {
                signal: self.signal,
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalId {
    #[serde(rename = "browser.tab_switch_count")]
    BrowserTabSwitchCount,
    #[serde(rename = "browser.open_tab_count_min")]
    BrowserOpenTabCountMin,
    #[serde(rename = "browser.open_tab_count_max")]
    BrowserOpenTabCountMax,
    #[serde(rename = "browser.open_tab_count_mean")]
    BrowserOpenTabCountMean,
    #[serde(rename = "browser.tab_open_count")]
    BrowserTabOpenCount,
    #[serde(rename = "browser.tab_close_count")]
    BrowserTabCloseCount,
    #[serde(rename = "browser.window_count_max")]
    BrowserWindowCountMax,
    #[serde(rename = "browser.active_seconds")]
    BrowserActiveSeconds,
    #[serde(rename = "browser.idle_seconds")]
    BrowserIdleSeconds,
    #[serde(rename = "browser.retained_across_day_count")]
    BrowserRetainedAcrossDayCount,
    #[serde(rename = "browser.continuous_scrolling_seconds")]
    BrowserContinuousScrollingSeconds,
    #[serde(rename = "os.active_seconds")]
    OsActiveSeconds,
    #[serde(rename = "os.idle_seconds")]
    OsIdleSeconds,
    #[serde(rename = "os.lock_count")]
    OsLockCount,
    #[serde(rename = "os.unlock_count")]
    OsUnlockCount,
    #[serde(rename = "os.suspend_count")]
    OsSuspendCount,
    #[serde(rename = "os.resume_count")]
    OsResumeCount,
}

impl SignalId {
    pub const ALL: [Self; 17] = [
        Self::BrowserTabSwitchCount,
        Self::BrowserOpenTabCountMin,
        Self::BrowserOpenTabCountMax,
        Self::BrowserOpenTabCountMean,
        Self::BrowserTabOpenCount,
        Self::BrowserTabCloseCount,
        Self::BrowserWindowCountMax,
        Self::BrowserActiveSeconds,
        Self::BrowserIdleSeconds,
        Self::BrowserRetainedAcrossDayCount,
        Self::BrowserContinuousScrollingSeconds,
        Self::OsActiveSeconds,
        Self::OsIdleSeconds,
        Self::OsLockCount,
        Self::OsUnlockCount,
        Self::OsSuspendCount,
        Self::OsResumeCount,
    ];

    pub const BROWSER_STARTER_SET: [Self; 7] = [
        Self::BrowserTabSwitchCount,
        Self::BrowserOpenTabCountMax,
        Self::BrowserOpenTabCountMean,
        Self::BrowserWindowCountMax,
        Self::BrowserActiveSeconds,
        Self::BrowserIdleSeconds,
        Self::BrowserRetainedAcrossDayCount,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BrowserTabSwitchCount => "browser.tab_switch_count",
            Self::BrowserOpenTabCountMin => "browser.open_tab_count_min",
            Self::BrowserOpenTabCountMax => "browser.open_tab_count_max",
            Self::BrowserOpenTabCountMean => "browser.open_tab_count_mean",
            Self::BrowserTabOpenCount => "browser.tab_open_count",
            Self::BrowserTabCloseCount => "browser.tab_close_count",
            Self::BrowserWindowCountMax => "browser.window_count_max",
            Self::BrowserActiveSeconds => "browser.active_seconds",
            Self::BrowserIdleSeconds => "browser.idle_seconds",
            Self::BrowserRetainedAcrossDayCount => "browser.retained_across_day_count",
            Self::BrowserContinuousScrollingSeconds => "browser.continuous_scrolling_seconds",
            Self::OsActiveSeconds => "os.active_seconds",
            Self::OsIdleSeconds => "os.idle_seconds",
            Self::OsLockCount => "os.lock_count",
            Self::OsUnlockCount => "os.unlock_count",
            Self::OsSuspendCount => "os.suspend_count",
            Self::OsResumeCount => "os.resume_count",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|signal| signal.as_str() == value)
    }

    pub const fn source_type(self) -> SourceType {
        match self {
            Self::BrowserTabSwitchCount
            | Self::BrowserOpenTabCountMin
            | Self::BrowserOpenTabCountMax
            | Self::BrowserOpenTabCountMean
            | Self::BrowserTabOpenCount
            | Self::BrowserTabCloseCount
            | Self::BrowserWindowCountMax
            | Self::BrowserActiveSeconds
            | Self::BrowserIdleSeconds
            | Self::BrowserRetainedAcrossDayCount
            | Self::BrowserContinuousScrollingSeconds => SourceType::Browser,
            Self::OsActiveSeconds
            | Self::OsIdleSeconds
            | Self::OsLockCount
            | Self::OsUnlockCount
            | Self::OsSuspendCount
            | Self::OsResumeCount => SourceType::Os,
        }
    }

    const fn kind(self) -> SignalKind {
        match self {
            Self::BrowserOpenTabCountMean => SignalKind::Mean,
            Self::BrowserActiveSeconds
            | Self::BrowserIdleSeconds
            | Self::BrowserContinuousScrollingSeconds
            | Self::OsActiveSeconds
            | Self::OsIdleSeconds => SignalKind::Duration,
            _ => SignalKind::Count,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SignalKind {
    Count,
    Duration,
    Mean,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("unsupported protocol version {received}; supported version is {supported}")]
    UnsupportedProtocolVersion { received: u16, supported: u16 },
    #[error("observation period end must be after start")]
    InvalidPeriod,
    #[error("observation period exceeds the maximum duration")]
    PeriodTooLong,
    #[error("observation period is too far in the future")]
    PeriodInFuture,
    #[error("time zone must be a valid IANA identifier")]
    InvalidTimeZone,
    #[error("aggregate batch must contain at least one metric")]
    EmptyMetrics,
    #[error("aggregate sequence exceeds the cross-platform safe integer range")]
    SequenceTooLarge,
    #[error("aggregate batch has {count} metrics; maximum is {maximum}")]
    TooManyMetrics { count: usize, maximum: usize },
    #[error("metric values must be finite")]
    NonFiniteMetric,
    #[error("invalid value for signal {signal:?}")]
    InvalidMetricValue { signal: SignalId },
    #[error("check-in timestamp is too far in the future")]
    CheckInInFuture,
    #[error("annotation timestamp is too far in the future")]
    AnnotationInFuture,
    #[error("local date must be an ISO date")]
    InvalidLocalDate,
    #[error("local date must match occurred_at in the supplied time zone")]
    LocalDateMismatch,
    #[error("sleep duration must be between 0 and 1440 minutes")]
    InvalidSleepMinutes,
    #[error("check-in scale values must be between 1 and 7")]
    InvalidCheckInScale,
    #[error("check-in has {count} context tags; maximum is {maximum}")]
    TooManyContextTags { count: usize, maximum: usize },
    #[error("context tag {tag:?} was repeated")]
    DuplicateContextTag { tag: ContextTag },
    #[error("check-in must contain at least one answered field or context tag")]
    EmptyCheckIn,
    #[error("annotation text must contain between 1 and 1000 bytes")]
    InvalidAnnotationText,
    #[error("annotation time window must be a same-day start/end pair")]
    InvalidAnnotationWindow,
    #[error("daily rhythm insight limit must be between 1 and {maximum}")]
    InvalidInsightLimit { maximum: u16 },
    #[error("daily timeline limit must be between 1 and {maximum}")]
    InvalidTimelineLimit { maximum: u16 },
    #[error("export directory must be a non-empty path")]
    InvalidExportDirectory,
    #[error("backup path must be a non-empty path")]
    InvalidBackupPath,
    #[error("recovery secret must be non-empty")]
    InvalidRecoverySecret,
}

pub fn typescript_schema() -> String {
    let mut output =
        String::from("// This file is generated by mindcanary-protocol. Do not edit by hand.\n\n");

    output.push_str("export const PROTOCOL_VERSION = 1 as const;\n\n");
    let _ = writeln!(
        output,
        "export const DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT = {DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT} as const;"
    );
    let _ = writeln!(
        output,
        "export const MAX_DAILY_RHYTHM_INSIGHT_LIMIT = {MAX_DAILY_RHYTHM_INSIGHT_LIMIT} as const;\n"
    );
    let _ = writeln!(
        output,
        "export const DEFAULT_DAILY_TIMELINE_LIMIT = {DEFAULT_DAILY_TIMELINE_LIMIT} as const;"
    );
    let _ = writeln!(
        output,
        "export const MAX_DAILY_TIMELINE_LIMIT = {MAX_DAILY_TIMELINE_LIMIT} as const;\n"
    );
    let _ = writeln!(
        output,
        "export const MIN_CHECK_IN_SCALE = {MIN_CHECK_IN_SCALE} as const;"
    );
    let _ = writeln!(
        output,
        "export const MAX_CHECK_IN_SCALE = {MAX_CHECK_IN_SCALE} as const;"
    );
    let _ = writeln!(
        output,
        "export const MAX_SLEEP_MINUTES = {MAX_SLEEP_MINUTES} as const;"
    );
    let _ = writeln!(
        output,
        "export const MAX_CONTEXT_TAGS = {MAX_CONTEXT_TAGS} as const;\n"
    );
    let _ = writeln!(
        output,
        "export const MAX_ANNOTATION_TEXT_BYTES = {MAX_ANNOTATION_TEXT_BYTES} as const;\n"
    );

    output.push_str("export const SIGNAL_IDS = [\n");
    for signal in SignalId::ALL {
        let _ = writeln!(output, "  \"{}\",", signal.as_str());
    }
    output.push_str("] as const;\n\n");
    output.push_str("export type SignalId = (typeof SIGNAL_IDS)[number];\n\n");
    output.push_str("export const BROWSER_STARTER_SIGNAL_IDS = [\n");
    for signal in SignalId::BROWSER_STARTER_SET {
        let _ = writeln!(output, "  \"{}\",", signal.as_str());
    }
    output.push_str("] as const satisfies readonly SignalId[];\n\n");
    output.push_str("export const CONTEXT_TAGS = [\n");
    for tag in ContextTag::ALL {
        let _ = writeln!(output, "  \"{}\",", tag.as_str());
    }
    output.push_str("] as const;\n\n");
    output.push_str("export type ContextTag = (typeof CONTEXT_TAGS)[number];\n\n");
    output.push_str(TYPESCRIPT_RECORD_TYPES);
    output.push_str(TYPESCRIPT_PROTOCOL_TYPES);

    output
}

const TYPESCRIPT_RECORD_TYPES: &str = r#"export interface ObservationPeriod {
  start: string;
  end: string;
  time_zone: string;
}

export interface Metric {
  signal: SignalId;
  value: number;
}

export interface AggregateBatch {
  batch_id: string;
  source_instance_id: string;
  sequence: number;
  period: ObservationPeriod;
  metrics: Metric[];
}

export interface CheckInRecord {
  check_in_id: string;
  occurred_at: string;
  time_zone: string;
  local_date: string;
  sleep_minutes?: number | null;
  perceived_sleep_need?: number | null;
  mood?: number | null;
  energy?: number | null;
  irritability?: number | null;
  concentration?: number | null;
  impulsivity?: number | null;
  medication_taken?: boolean | null;
  substance_use?: boolean | null;
  context_tags: ContextTag[];
}

export interface AnnotationRecord {
  annotation_id: string;
  created_at: string;
  time_zone: string;
  local_date: string;
  start_minute?: number | null;
  end_minute?: number | null;
  text: string;
  context_tags: ContextTag[];
}

export interface DailyRhythmSummary {
  daily_snapshot_count: number;
  browser_day_count: number;
  os_day_count: number;
  check_in_day_count: number;
  insight_count_before_limit: number;
  insights_truncated: boolean;
}

export type RhythmInsightDimension =
  | "browser_tabs"
  | "tab_switching"
  | "active_time"
  | "computer_active_time"
  | "sleep"
  | "energy";

export type RhythmChangeDirection = "higher" | "lower";

export interface RhythmEvidence {
  label: string;
  value: string;
}

export interface RhythmInsight {
  local_date: string;
  dimension: RhythmInsightDimension;
  direction: RhythmChangeDirection;
  summary: string;
  evidence: RhythmEvidence[];
}

export interface RhythmDimensionReadiness {
  dimension: RhythmInsightDimension;
  status: RhythmReadinessStatus;
  comparable_day_count: number;
  minimum_day_count: number;
}

export type RhythmReadinessStatus =
  | "change_described"
  | "within_baseline"
  | "needs_sustained_change"
  | "missing_current"
  | "insufficient_baseline"
  | "zero_baseline"
  | "unstable_baseline";

export interface DailyTimelineSummary {
  calendar_day_count_before_limit: number;
  returned_day_count: number;
  browser_day_count: number;
  os_day_count: number;
  check_in_day_count: number;
  annotation_day_count: number;
  missing_day_count: number;
  days_truncated: boolean;
}

export interface DailyBrowserTimeline {
  open_tab_count_mean?: number | null;
  open_tab_count_max?: number | null;
  tab_switch_count?: number | null;
  retained_across_day_count?: number | null;
  continuous_scrolling_seconds?: number | null;
  active_seconds?: number | null;
  idle_seconds?: number | null;
  recorded_bucket_count: number;
}

export interface DailyOsTimeline {
  active_seconds?: number | null;
  idle_seconds?: number | null;
  lock_count?: number | null;
  unlock_count?: number | null;
  suspend_count?: number | null;
  resume_count?: number | null;
  recorded_bucket_count: number;
}

export interface DailyCheckInTimeline {
  sleep_minutes?: number | null;
  mood?: number | null;
  energy?: number | null;
  irritability?: number | null;
  concentration?: number | null;
  impulsivity?: number | null;
  check_in_count: number;
  context_tags: ContextTag[];
}

export interface DailyTimelineDay {
  local_date: string;
  browser?: DailyBrowserTimeline | null;
  os?: DailyOsTimeline | null;
  check_in?: DailyCheckInTimeline | null;
  annotations: AnnotationRecord[];
}

export interface LocalDataSummary {
  aggregate_batch_count: number;
  aggregate_metric_count: number;
  check_in_count: number;
  context_tag_count: number;
  annotation_count: number;
  annotation_context_tag_count: number;
}

export interface SignalCollectionSetting {
  signal: SignalId;
  enabled: boolean;
  changed_at?: string | null;
}

export interface SignalRecordSummary {
  metric_record_count: number;
  affected_batch_count: number;
}

export interface LocalDataExport {
  export_directory: string;
  report_path: string;
  daily_browser_csv_path: string;
  daily_os_csv_path: string;
  daily_check_in_csv_path: string;
  annotations_csv_path: string;
  summary: LocalDataSummary;
}

export interface LocalBackup {
  backup_path: string;
  created_at: string;
  format_version: number;
  schema_version: number;
  recovery_secret: string;
  summary: LocalDataSummary;
}

export interface LocalBackupMetadata {
  backup_path: string;
  created_at: string;
  format_version: number;
  schema_version: number;
}

export type OperatingSystem = "linux" | "macos" | "windows" | "other";

export type DesktopEnvironment = "gnome" | "kde" | "other" | "unknown";

export type SessionType = "x11" | "wayland" | "other" | "unknown";

export type PlatformCapabilityId =
  | "os_lock_and_session_events"
  | "os_active_idle_duration"
  | "foreground_application_category";

export type PlatformCapabilityStatus = "available" | "planned" | "unavailable";

export interface PlatformCapability {
  capability: PlatformCapabilityId;
  status: PlatformCapabilityStatus;
  detail: string;
}

export interface PlatformCapabilities {
  operating_system: OperatingSystem;
  desktop_environment: DesktopEnvironment;
  session_type: SessionType;
  capabilities: PlatformCapability[];
}

"#;

const TYPESCRIPT_PROTOCOL_TYPES: &str = r#"export type ProtocolRequest =
  | { type: "health"; protocol_version: typeof PROTOCOL_VERSION }
  | { type: "get_source_status"; protocol_version: typeof PROTOCOL_VERSION }
  | {
      type: "ingest_aggregate";
      protocol_version: typeof PROTOCOL_VERSION;
      batch: AggregateBatch;
    }
  | {
      type: "submit_check_in";
      protocol_version: typeof PROTOCOL_VERSION;
      check_in: CheckInRecord;
    }
  | {
      type: "prepare_delete_latest_check_in";
      protocol_version: typeof PROTOCOL_VERSION;
      local_date: string;
    }
  | {
      type: "delete_latest_check_in";
      protocol_version: typeof PROTOCOL_VERSION;
      local_date: string;
      confirmation_token: string;
    }
  | {
      type: "save_annotation";
      protocol_version: typeof PROTOCOL_VERSION;
      annotation: AnnotationRecord;
    }
  | {
      type: "prepare_delete_annotation";
      protocol_version: typeof PROTOCOL_VERSION;
      annotation_id: string;
    }
  | {
      type: "delete_annotation";
      protocol_version: typeof PROTOCOL_VERSION;
      annotation_id: string;
      confirmation_token: string;
    }
  | {
      type: "get_daily_rhythm_insights";
      protocol_version: typeof PROTOCOL_VERSION;
      limit?: number | null;
    }
  | {
      type: "get_daily_timeline";
      protocol_version: typeof PROTOCOL_VERSION;
      limit?: number | null;
    }
  | {
      type: "get_collection_settings";
      protocol_version: typeof PROTOCOL_VERSION;
    }
  | {
      type: "get_platform_capabilities";
      protocol_version: typeof PROTOCOL_VERSION;
    }
  | {
      type: "set_signal_collection";
      protocol_version: typeof PROTOCOL_VERSION;
      signal: SignalId;
      enabled: boolean;
    }
  | {
      type: "prepare_delete_signal_records";
      protocol_version: typeof PROTOCOL_VERSION;
      signal: SignalId;
    }
  | {
      type: "delete_signal_records";
      protocol_version: typeof PROTOCOL_VERSION;
      signal: SignalId;
      confirmation_token: string;
    }
  | {
      type: "get_local_data_summary";
      protocol_version: typeof PROTOCOL_VERSION;
    }
  | {
      type: "prepare_export_local_records";
      protocol_version: typeof PROTOCOL_VERSION;
    }
  | {
      type: "export_local_records";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
      export_directory: string;
    }
  | {
      type: "prepare_create_local_backup";
      protocol_version: typeof PROTOCOL_VERSION;
    }
  | {
      type: "create_local_backup";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
      backup_path: string;
    }
  | {
      type: "verify_local_backup";
      protocol_version: typeof PROTOCOL_VERSION;
      backup_path: string;
      recovery_secret: string;
    }
  | {
      type: "restore_local_backup";
      protocol_version: typeof PROTOCOL_VERSION;
      backup_path: string;
      recovery_secret: string;
    }
  | {
      type: "prepare_clear_local_records";
      protocol_version: typeof PROTOCOL_VERSION;
    }
  | {
      type: "clear_local_records";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
    };

export type ServiceStatus = "ready" | "degraded";
export type SourceType = "browser" | "os" | "check_in";
export type SourceHealth =
  | "never_seen"
  | "active"
  | "stale"
  | "disabled"
  | "unavailable";

export interface SourceStatus {
  source: SourceType;
  health: SourceHealth;
  last_received_at?: string | null;
}

export type IngestDisposition =
  | "stored"
  | "stored_filtered"
  | "duplicate"
  | "discarded_disabled";
export type ErrorCode =
  | "invalid_frame"
  | "invalid_request"
  | "message_too_large"
  | "sequence_conflict"
  | "unsupported_protocol_version"
  | "invalid_confirmation"
  | "internal";

export type ProtocolResponse =
  | {
      type: "health";
      protocol_version: typeof PROTOCOL_VERSION;
      service_version: string;
      status: ServiceStatus;
    }
  | {
      type: "source_status";
      protocol_version: typeof PROTOCOL_VERSION;
      generated_at: string;
      sources: SourceStatus[];
    }
  | {
      type: "ingest_acknowledged";
      protocol_version: typeof PROTOCOL_VERSION;
      batch_id: string;
      disposition: IngestDisposition;
    }
  | {
      type: "check_in_acknowledged";
      protocol_version: typeof PROTOCOL_VERSION;
      check_in_id: string;
      disposition: IngestDisposition;
    }
  | {
      type: "delete_latest_check_in_confirmation";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
      expires_at: string;
      local_date: string;
      check_in_id: string;
    }
  | {
      type: "check_in_deleted";
      protocol_version: typeof PROTOCOL_VERSION;
      local_date: string;
      check_in_id: string;
    }
  | {
      type: "annotation_saved";
      protocol_version: typeof PROTOCOL_VERSION;
      annotation_id: string;
    }
  | {
      type: "delete_annotation_confirmation";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
      expires_at: string;
      annotation_id: string;
    }
  | {
      type: "annotation_deleted";
      protocol_version: typeof PROTOCOL_VERSION;
      annotation_id: string;
    }
  | {
      type: "daily_rhythm_insights";
      protocol_version: typeof PROTOCOL_VERSION;
      generated_at: string;
      summary: DailyRhythmSummary;
      readiness: RhythmDimensionReadiness[];
      insights: RhythmInsight[];
    }
  | {
      type: "daily_timeline";
      protocol_version: typeof PROTOCOL_VERSION;
      generated_at: string;
      summary: DailyTimelineSummary;
      days: DailyTimelineDay[];
    }
  | {
      type: "collection_settings";
      protocol_version: typeof PROTOCOL_VERSION;
      settings: SignalCollectionSetting[];
    }
  | {
      type: "platform_capabilities";
      protocol_version: typeof PROTOCOL_VERSION;
      capabilities: PlatformCapabilities;
    }
  | {
      type: "delete_signal_records_confirmation";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
      expires_at: string;
      signal: SignalId;
      summary: SignalRecordSummary;
    }
  | {
      type: "signal_records_deleted";
      protocol_version: typeof PROTOCOL_VERSION;
      signal: SignalId;
      deleted: SignalRecordSummary;
    }
  | {
      type: "local_data_summary";
      protocol_version: typeof PROTOCOL_VERSION;
      summary: LocalDataSummary;
    }
  | {
      type: "export_local_records_confirmation";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
      expires_at: string;
      summary: LocalDataSummary;
    }
  | {
      type: "local_records_exported";
      protocol_version: typeof PROTOCOL_VERSION;
      export: LocalDataExport;
    }
  | {
      type: "create_local_backup_confirmation";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
      expires_at: string;
      summary: LocalDataSummary;
    }
  | {
      type: "local_backup_created";
      protocol_version: typeof PROTOCOL_VERSION;
      backup: LocalBackup;
    }
  | {
      type: "local_backup_verified";
      protocol_version: typeof PROTOCOL_VERSION;
      backup: LocalBackupMetadata;
    }
  | {
      type: "local_backup_restored";
      protocol_version: typeof PROTOCOL_VERSION;
      backup: LocalBackupMetadata;
      restored: LocalDataSummary;
    }
  | {
      type: "clear_local_records_confirmation";
      protocol_version: typeof PROTOCOL_VERSION;
      confirmation_token: string;
      expires_at: string;
      summary: LocalDataSummary;
    }
  | {
      type: "local_records_cleared";
      protocol_version: typeof PROTOCOL_VERSION;
      deleted: LocalDataSummary;
    }
  | {
      type: "error";
      protocol_version: typeof PROTOCOL_VERSION;
      code: ErrorCode;
    };
"#;

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn valid_batch() -> AggregateBatch {
        AggregateBatch {
            batch_id: Uuid::now_v7(),
            source_instance_id: Uuid::now_v7(),
            sequence: 1,
            period: ObservationPeriod {
                start: Utc.with_ymd_and_hms(2026, 6, 14, 12, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 6, 14, 12, 15, 0).unwrap(),
                time_zone: "America/Sao_Paulo".to_owned(),
            },
            metrics: vec![Metric {
                signal: SignalId::BrowserTabSwitchCount,
                value: 18.0,
            }],
        }
    }

    fn valid_check_in() -> CheckInRecord {
        CheckInRecord {
            check_in_id: Uuid::now_v7(),
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
            context_tags: vec![ContextTag::Deadline, ContextTag::NewsCycle],
        }
    }

    fn valid_annotation() -> AnnotationRecord {
        AnnotationRecord {
            annotation_id: Uuid::now_v7(),
            created_at: Utc.with_ymd_and_hms(2026, 6, 14, 12, 10, 0).unwrap(),
            time_zone: "America/Sao_Paulo".to_owned(),
            local_date: "2026-06-14".to_owned(),
            start_minute: Some(8 * 60),
            end_minute: Some(9 * 60),
            text: "Power outage before breakfast".to_owned(),
            context_tags: vec![ContextTag::Other],
        }
    }

    #[test]
    fn accepts_a_valid_aggregate() {
        let request = ProtocolRequest::IngestAggregate {
            protocol_version: PROTOCOL_VERSION,
            batch: valid_batch(),
        };
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(request.validate_at(now), Ok(()));
    }

    #[test]
    fn accepts_a_valid_check_in() {
        let request = ProtocolRequest::SubmitCheckIn {
            protocol_version: PROTOCOL_VERSION,
            check_in: valid_check_in(),
        };
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(request.validate_at(now), Ok(()));
    }

    #[test]
    fn accepts_day_and_time_window_annotations() {
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();
        let window = ProtocolRequest::SaveAnnotation {
            protocol_version: PROTOCOL_VERSION,
            annotation: valid_annotation(),
        };
        assert_eq!(window.validate_at(now), Ok(()));

        let mut day = valid_annotation();
        day.start_minute = None;
        day.end_minute = None;
        day.text = "A deliberately quiet day".to_owned();
        let day = ProtocolRequest::SaveAnnotation {
            protocol_version: PROTOCOL_VERSION,
            annotation: day,
        };
        assert_eq!(day.validate_at(now), Ok(()));
    }

    #[test]
    fn rejects_empty_or_partial_annotation_windows() {
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();
        let mut annotation = valid_annotation();
        annotation.text = "  ".to_owned();
        assert_eq!(
            annotation.validate_at(now),
            Err(ValidationError::InvalidAnnotationText)
        );

        let mut annotation = valid_annotation();
        annotation.end_minute = None;
        assert_eq!(
            annotation.validate_at(now),
            Err(ValidationError::InvalidAnnotationWindow)
        );
    }

    #[test]
    fn accepts_a_valid_daily_rhythm_insight_request() {
        let request = ProtocolRequest::GetDailyRhythmInsights {
            protocol_version: PROTOCOL_VERSION,
            limit: Some(10),
        };
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(request.validate_at(now), Ok(()));
    }

    #[test]
    fn accepts_a_valid_daily_timeline_request() {
        let request = ProtocolRequest::GetDailyTimeline {
            protocol_version: PROTOCOL_VERSION,
            limit: Some(30),
        };
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(request.validate_at(now), Ok(()));
    }

    #[test]
    fn administrative_requests_are_not_collector_requests() {
        let requests = [
            ProtocolRequest::GetSourceStatus {
                protocol_version: PROTOCOL_VERSION,
            },
            ProtocolRequest::SetSignalCollection {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::BrowserTabSwitchCount,
                enabled: false,
            },
            ProtocolRequest::GetDailyRhythmInsights {
                protocol_version: PROTOCOL_VERSION,
                limit: None,
            },
            ProtocolRequest::GetDailyTimeline {
                protocol_version: PROTOCOL_VERSION,
                limit: None,
            },
            ProtocolRequest::GetPlatformCapabilities {
                protocol_version: PROTOCOL_VERSION,
            },
            ProtocolRequest::PrepareDeleteSignalRecords {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::BrowserTabSwitchCount,
            },
            ProtocolRequest::DeleteSignalRecords {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::BrowserTabSwitchCount,
                confirmation_token: Uuid::now_v7(),
            },
            ProtocolRequest::GetLocalDataSummary {
                protocol_version: PROTOCOL_VERSION,
            },
            ProtocolRequest::PrepareExportLocalRecords {
                protocol_version: PROTOCOL_VERSION,
            },
            ProtocolRequest::ExportLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token: Uuid::now_v7(),
                export_directory: "/tmp/mindcanary-export".to_owned(),
            },
            ProtocolRequest::PrepareCreateLocalBackup {
                protocol_version: PROTOCOL_VERSION,
            },
            ProtocolRequest::CreateLocalBackup {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token: Uuid::now_v7(),
                backup_path: "/tmp/mindcanary.mcbak".to_owned(),
            },
            ProtocolRequest::VerifyLocalBackup {
                protocol_version: PROTOCOL_VERSION,
                backup_path: "/tmp/mindcanary.mcbak".to_owned(),
                recovery_secret: "secret".to_owned(),
            },
            ProtocolRequest::RestoreLocalBackup {
                protocol_version: PROTOCOL_VERSION,
                backup_path: "/tmp/mindcanary.mcbak".to_owned(),
                recovery_secret: "secret".to_owned(),
            },
            ProtocolRequest::PrepareClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
            },
            ProtocolRequest::ClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token: Uuid::now_v7(),
            },
        ];

        for request in requests {
            assert!(!request.is_collector_request());
        }
        assert!(
            ProtocolRequest::Health {
                protocol_version: PROTOCOL_VERSION
            }
            .is_collector_request()
        );
        assert!(
            ProtocolRequest::IngestAggregate {
                protocol_version: PROTOCOL_VERSION,
                batch: valid_batch()
            }
            .is_collector_request()
        );
        assert!(
            ProtocolRequest::GetCollectionSettings {
                protocol_version: PROTOCOL_VERSION
            }
            .is_collector_request()
        );
    }

    #[test]
    fn rejects_invalid_daily_rhythm_insight_limits() {
        let request = ProtocolRequest::GetDailyRhythmInsights {
            protocol_version: PROTOCOL_VERSION,
            limit: Some(0),
        };
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(
            request.validate_at(now),
            Err(ValidationError::InvalidInsightLimit {
                maximum: MAX_DAILY_RHYTHM_INSIGHT_LIMIT
            })
        );
    }

    #[test]
    fn rejects_invalid_daily_timeline_limits() {
        let request = ProtocolRequest::GetDailyTimeline {
            protocol_version: PROTOCOL_VERSION,
            limit: Some(MAX_DAILY_TIMELINE_LIMIT + 1),
        };
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(
            request.validate_at(now),
            Err(ValidationError::InvalidTimelineLimit {
                maximum: MAX_DAILY_TIMELINE_LIMIT
            })
        );
    }

    #[test]
    fn rejects_invalid_export_directories() {
        let request = ProtocolRequest::ExportLocalRecords {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token: Uuid::now_v7(),
            export_directory: "   ".to_owned(),
        };
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(
            request.validate_at(now),
            Err(ValidationError::InvalidExportDirectory)
        );
    }

    #[test]
    fn decodes_pre_annotation_exports_during_local_upgrades() {
        let response: ProtocolResponse = serde_json::from_value(json!({
            "type": "local_records_exported",
            "protocol_version": PROTOCOL_VERSION,
            "export": {
                "export_directory": "/tmp/mindcanary-export",
                "report_path": "/tmp/mindcanary-export/mindcanary-report.md",
                "daily_browser_csv_path": "/tmp/mindcanary-export/daily-browser.csv",
                "daily_os_csv_path": "/tmp/mindcanary-export/daily-os.csv",
                "daily_check_in_csv_path": "/tmp/mindcanary-export/daily-check-ins.csv",
                "summary": {
                    "aggregate_batch_count": 4,
                    "aggregate_metric_count": 8,
                    "check_in_count": 2,
                    "context_tag_count": 1
                }
            }
        }))
        .unwrap();

        let ProtocolResponse::LocalRecordsExported { export, .. } = response else {
            panic!("expected local export response");
        };
        assert_eq!(export.annotations_csv_path, "");
        assert_eq!(export.summary.annotation_count, 0);
        assert_eq!(export.summary.annotation_context_tag_count, 0);
    }

    #[test]
    fn rejects_empty_backup_paths_and_recovery_secrets() {
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();
        assert_eq!(
            ProtocolRequest::CreateLocalBackup {
                protocol_version: PROTOCOL_VERSION,
                confirmation_token: Uuid::now_v7(),
                backup_path: " ".to_owned(),
            }
            .validate_at(now),
            Err(ValidationError::InvalidBackupPath)
        );
        assert_eq!(
            ProtocolRequest::VerifyLocalBackup {
                protocol_version: PROTOCOL_VERSION,
                backup_path: "/tmp/backup.mcbak".to_owned(),
                recovery_secret: String::new(),
            }
            .validate_at(now),
            Err(ValidationError::InvalidRecoverySecret)
        );
    }

    #[test]
    fn rejects_unknown_fields_including_urls() {
        let value = json!({
            "type": "ingest_aggregate",
            "protocol_version": 1,
            "batch": {
                "batch_id": Uuid::now_v7(),
                "source_instance_id": Uuid::now_v7(),
                "sequence": 1,
                "period": {
                    "start": "2026-06-14T12:00:00Z",
                    "end": "2026-06-14T12:15:00Z",
                    "time_zone": "America/Sao_Paulo"
                },
                "metrics": [{
                    "signal": "browser.tab_switch_count",
                    "value": 18,
                    "url": "https://example.com/private"
                }]
            }
        });

        assert!(serde_json::from_value::<ProtocolRequest>(value).is_err());
    }

    #[test]
    fn rejects_unknown_signal_ids() {
        let value = json!({
            "type": "ingest_aggregate",
            "protocol_version": 1,
            "batch": {
                "batch_id": Uuid::now_v7(),
                "source_instance_id": Uuid::now_v7(),
                "sequence": 1,
                "period": {
                    "start": "2026-06-14T12:00:00Z",
                    "end": "2026-06-14T12:15:00Z",
                    "time_zone": "America/Sao_Paulo"
                },
                "metrics": [{
                    "signal": "browser.raw_url",
                    "value": 1
                }]
            }
        });

        assert!(serde_json::from_value::<ProtocolRequest>(value).is_err());
    }

    #[test]
    fn rejects_unknown_check_in_fields_including_notes_and_diagnoses() {
        let mut value = serde_json::to_value(valid_check_in()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.insert("note".to_owned(), json!("raw private note"));
        object.insert("diagnosis".to_owned(), json!("mania"));

        let request = json!({
            "type": "submit_check_in",
            "protocol_version": 1,
            "check_in": value
        });

        assert!(serde_json::from_value::<ProtocolRequest>(request).is_err());
    }

    #[test]
    fn rejects_invalid_check_in_scale_values() {
        let mut check_in = valid_check_in();
        check_in.energy = Some(8);
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(
            check_in.validate_at(now),
            Err(ValidationError::InvalidCheckInScale)
        );
    }

    #[test]
    fn rejects_duplicate_context_tags() {
        let mut check_in = valid_check_in();
        check_in.context_tags = vec![ContextTag::Deadline, ContextTag::Deadline];
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(
            check_in.validate_at(now),
            Err(ValidationError::DuplicateContextTag {
                tag: ContextTag::Deadline
            })
        );
    }

    #[test]
    fn rejects_durations_longer_than_the_period() {
        let mut batch = valid_batch();
        batch.metrics = vec![Metric {
            signal: SignalId::BrowserActiveSeconds,
            value: 901.0,
        }];
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();

        assert_eq!(
            batch.validate_at(now),
            Err(ValidationError::InvalidMetricValue {
                signal: SignalId::BrowserActiveSeconds
            })
        );
    }

    #[test]
    fn generated_types_include_every_signal() {
        let schema = typescript_schema();

        for signal in SignalId::ALL {
            assert!(schema.contains(signal.as_str()));
        }

        for tag in ContextTag::ALL {
            assert!(schema.contains(tag.as_str()));
        }

        for signal in SignalId::BROWSER_STARTER_SET {
            assert!(schema.contains(signal.as_str()));
            assert!(signal.as_str().starts_with("browser."));
        }

        assert!(!schema.contains("mania"));
        assert!(!schema.contains("depression"));
        assert!(!schema.contains("psychosis"));
        assert!(!schema.contains("diagnosis"));
    }
}
