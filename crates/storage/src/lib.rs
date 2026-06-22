use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, OpenOptions},
    io::ErrorKind,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use mindcanary_protocol::{
    AggregateBatch, AnnotationRecord, CheckInRecord, ContextTag, IngestDisposition,
    SignalCollectionSetting, SignalId, SignalRecordSummary, SourceType,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use thiserror::Error;
use uuid::Uuid;
use zeroize::ZeroizeOnDrop;

const DATABASE_KEY_BYTES: usize = 32;
const CURRENT_SCHEMA_VERSION: i64 = 4;
pub const BACKUP_FORMAT_VERSION: i64 = 1;
const KEYRING_SERVICE: &str = "app.mindcanary.database";
const KEYRING_USER: &str = "primary-v1";

#[derive(Clone, ZeroizeOnDrop)]
pub struct DatabaseKey([u8; DATABASE_KEY_BYTES]);

impl DatabaseKey {
    pub fn generate() -> Result<Self, StorageError> {
        let mut bytes = [0_u8; DATABASE_KEY_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| StorageError::KeyGeneration)?;
        Ok(Self(bytes))
    }

    pub fn from_bytes(bytes: [u8; DATABASE_KEY_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, StorageError> {
        let bytes: [u8; DATABASE_KEY_BYTES] = bytes
            .try_into()
            .map_err(|_| StorageError::InvalidKeyLength)?;
        Ok(Self(bytes))
    }

    fn sqlcipher_literal(&self) -> zeroize::Zeroizing<String> {
        use fmt::Write as _;

        let mut literal =
            zeroize::Zeroizing::new(String::with_capacity(2 * DATABASE_KEY_BYTES + 3));
        literal.push_str("x'");
        for byte in self.0 {
            let _ = write!(literal, "{byte:02x}");
        }
        literal.push('\'');
        literal
    }

    pub fn as_bytes(&self) -> &[u8; DATABASE_KEY_BYTES] {
        &self.0
    }
}

impl fmt::Debug for DatabaseKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseKey([REDACTED])")
    }
}

pub trait DatabaseKeyProvider: Send + Sync {
    fn load(&self) -> Result<Option<DatabaseKey>, StorageError>;
    fn store(&self, key: &DatabaseKey) -> Result<(), StorageError>;
    fn delete(&self) -> Result<(), StorageError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OsKeyringKeyProvider;

impl OsKeyringKeyProvider {
    fn entry() -> Result<keyring::Entry, StorageError> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(StorageError::Keyring)
    }
}

impl DatabaseKeyProvider for OsKeyringKeyProvider {
    fn load(&self) -> Result<Option<DatabaseKey>, StorageError> {
        match Self::entry()?.get_secret() {
            Ok(secret) => DatabaseKey::try_from_slice(&secret).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(StorageError::Keyring(error)),
        }
    }

    fn store(&self, key: &DatabaseKey) -> Result<(), StorageError> {
        Self::entry()?
            .set_secret(key.as_bytes())
            .map_err(StorageError::Keyring)
    }

    fn delete(&self) -> Result<(), StorageError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(StorageError::Keyring(error)),
        }
    }
}

#[derive(Debug)]
pub struct EncryptedStore {
    connection: Connection,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DailyBrowserFeatures {
    pub local_date: String,
    pub open_tab_count_mean: Option<f64>,
    pub open_tab_count_max: Option<f64>,
    pub tab_switch_count: Option<f64>,
    pub retained_across_day_count: Option<f64>,
    pub continuous_scrolling_seconds: Option<f64>,
    pub active_seconds: Option<f64>,
    pub idle_seconds: Option<f64>,
    pub aggregate_bucket_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DailyOsFeatures {
    pub local_date: String,
    pub active_seconds: Option<f64>,
    pub idle_seconds: Option<f64>,
    pub lock_count: Option<f64>,
    pub unlock_count: Option<f64>,
    pub suspend_count: Option<f64>,
    pub resume_count: Option<f64>,
    pub aggregate_bucket_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DailyCheckInFeatures {
    pub local_date: String,
    pub sleep_minutes: Option<f64>,
    pub mood: Option<f64>,
    pub energy: Option<f64>,
    pub irritability: Option<f64>,
    pub concentration: Option<f64>,
    pub impulsivity: Option<f64>,
    pub check_in_count: u64,
    pub context_tags: Vec<ContextTag>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceActivityTimestamps {
    pub browser: Option<DateTime<Utc>>,
    pub os: Option<DateTime<Utc>>,
    pub check_in: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProfileDestroyReport {
    pub database_path: PathBuf,
    pub removed_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedBackup {
    pub path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub format_version: i64,
    pub schema_version: i64,
    pub recovery_secret: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedBackupMetadata {
    pub created_at: DateTime<Utc>,
    pub format_version: i64,
    pub schema_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupRestoreReport {
    pub aggregate_batch_count: u64,
    pub aggregate_metric_count: u64,
    pub check_in_count: u64,
    pub annotation_count: u64,
}

fn latest_timestamp(current: Option<DateTime<Utc>>, candidate: DateTime<Utc>) -> DateTime<Utc> {
    current.map_or(candidate, |current| current.max(candidate))
}

impl EncryptedStore {
    pub fn bootstrap(
        path: impl AsRef<Path>,
        provider: &dyn DatabaseKeyProvider,
    ) -> Result<Self, StorageError> {
        let path = path.as_ref();
        let database_has_content = path.metadata().is_ok_and(|metadata| metadata.len() > 0);

        let (key, newly_stored) = match provider.load()? {
            Some(key) => (key, false),
            None if database_has_content => return Err(StorageError::MissingDatabaseKey),
            None => {
                let key = DatabaseKey::generate()?;
                provider.store(&key)?;
                (key, true)
            }
        };

        match Self::open(path, &key) {
            Ok(store) => Ok(store),
            Err(error) if newly_stored => {
                let _ = provider.delete();
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    pub fn open(path: impl AsRef<Path>, key: &DatabaseKey) -> Result<Self, StorageError> {
        let path = path.as_ref();
        prepare_database_parent(path)?;
        ensure_private_database_file(path)?;

        let mut connection = Connection::open(path).map_err(StorageError::Database)?;
        apply_key(&connection, key)?;
        verify_sqlcipher(&connection)?;
        configure_connection(&connection)?;
        migrate(&mut connection)?;

        Ok(Self {
            connection,
            path: path.to_owned(),
        })
    }

    pub fn ingest(&mut self, batch: &AggregateBatch) -> Result<IngestDisposition, StorageError> {
        self.ingest_at(batch, Utc::now())
    }

    pub fn ingest_at(
        &mut self,
        batch: &AggregateBatch,
        received_at: DateTime<Utc>,
    ) -> Result<IngestDisposition, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::Database)?;

        let duplicate = transaction
            .query_row(
                "SELECT 1 FROM aggregate_batches WHERE batch_id = ?1",
                [batch.batch_id.as_bytes().as_slice()],
                |_| Ok(()),
            )
            .optional()
            .map_err(StorageError::Database)?
            .is_some();

        if duplicate {
            transaction.commit().map_err(StorageError::Database)?;
            return Ok(IngestDisposition::Duplicate);
        }

        let last_sequence = transaction
            .query_row(
                "SELECT last_sequence FROM source_sequences WHERE source_instance_id = ?1",
                [batch.source_instance_id.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(StorageError::Database)?;
        let sequence =
            i64::try_from(batch.sequence).map_err(|_| StorageError::SequenceOutOfRange)?;

        if last_sequence.is_some_and(|last| last >= sequence) {
            return Err(StorageError::SequenceConflict);
        }

        transaction
            .execute(
                "INSERT INTO aggregate_batches (
                    batch_id,
                    source_instance_id,
                    source_sequence,
                    period_start_ms,
                    period_end_ms,
                    time_zone,
                    received_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    batch.batch_id.as_bytes().as_slice(),
                    batch.source_instance_id.as_bytes().as_slice(),
                    sequence,
                    batch.period.start.timestamp_millis(),
                    batch.period.end.timestamp_millis(),
                    batch.period.time_zone,
                    received_at.timestamp_millis(),
                ],
            )
            .map_err(StorageError::Database)?;

        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO aggregate_metrics (batch_id, signal_id, value)
                     VALUES (?1, ?2, ?3)",
                )
                .map_err(StorageError::Database)?;

            for metric in &batch.metrics {
                statement
                    .execute(params![
                        batch.batch_id.as_bytes().as_slice(),
                        metric.signal.as_str(),
                        metric.value
                    ])
                    .map_err(StorageError::Database)?;
            }
        }

        transaction
            .execute(
                "INSERT INTO source_sequences (source_instance_id, last_sequence)
                 VALUES (?1, ?2)
                 ON CONFLICT(source_instance_id)
                 DO UPDATE SET last_sequence = excluded.last_sequence",
                params![batch.source_instance_id.as_bytes().as_slice(), sequence],
            )
            .map_err(StorageError::Database)?;

        transaction.commit().map_err(StorageError::Database)?;
        Ok(IngestDisposition::Stored)
    }

    pub fn submit_check_in(
        &mut self,
        check_in: &CheckInRecord,
    ) -> Result<IngestDisposition, StorageError> {
        self.submit_check_in_at(check_in, Utc::now())
    }

    pub fn submit_check_in_at(
        &mut self,
        check_in: &CheckInRecord,
        received_at: DateTime<Utc>,
    ) -> Result<IngestDisposition, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::Database)?;

        let duplicate = transaction
            .query_row(
                "SELECT 1 FROM check_ins WHERE check_in_id = ?1",
                [check_in.check_in_id.as_bytes().as_slice()],
                |_| Ok(()),
            )
            .optional()
            .map_err(StorageError::Database)?
            .is_some();

        if duplicate {
            transaction.commit().map_err(StorageError::Database)?;
            return Ok(IngestDisposition::Duplicate);
        }

        transaction
            .execute(
                "INSERT INTO check_ins (
                    check_in_id,
                    occurred_at_ms,
                    time_zone,
                    local_date,
                    sleep_minutes,
                    perceived_sleep_need,
                    mood,
                    energy,
                    irritability,
                    concentration,
                    impulsivity,
                    medication_taken,
                    substance_use,
                    created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    check_in.check_in_id.as_bytes().as_slice(),
                    check_in.occurred_at.timestamp_millis(),
                    check_in.time_zone,
                    check_in.local_date,
                    check_in.sleep_minutes.map(i64::from),
                    check_in.perceived_sleep_need.map(i64::from),
                    check_in.mood.map(i64::from),
                    check_in.energy.map(i64::from),
                    check_in.irritability.map(i64::from),
                    check_in.concentration.map(i64::from),
                    check_in.impulsivity.map(i64::from),
                    optional_bool_to_i64(check_in.medication_taken),
                    optional_bool_to_i64(check_in.substance_use),
                    received_at.timestamp_millis(),
                ],
            )
            .map_err(StorageError::Database)?;

        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO check_in_context_tags (check_in_id, tag)
                     VALUES (?1, ?2)",
                )
                .map_err(StorageError::Database)?;

            for tag in &check_in.context_tags {
                statement
                    .execute(params![
                        check_in.check_in_id.as_bytes().as_slice(),
                        tag.as_str()
                    ])
                    .map_err(StorageError::Database)?;
            }
        }

        transaction.commit().map_err(StorageError::Database)?;
        Ok(IngestDisposition::Stored)
    }

    pub fn save_annotation(&mut self, annotation: &AnnotationRecord) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::Database)?;

        transaction
            .execute(
                "INSERT INTO annotations (
                    annotation_id,
                    created_at_ms,
                    time_zone,
                    local_date,
                    start_minute,
                    end_minute,
                    text
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(annotation_id)
                 DO UPDATE SET
                    time_zone = excluded.time_zone,
                    local_date = excluded.local_date,
                    start_minute = excluded.start_minute,
                    end_minute = excluded.end_minute,
                    text = excluded.text",
                params![
                    annotation.annotation_id.as_bytes().as_slice(),
                    annotation.created_at.timestamp_millis(),
                    annotation.time_zone,
                    annotation.local_date,
                    annotation.start_minute.map(i64::from),
                    annotation.end_minute.map(i64::from),
                    annotation.text,
                ],
            )
            .map_err(StorageError::Database)?;
        transaction
            .execute(
                "DELETE FROM annotation_context_tags WHERE annotation_id = ?1",
                [annotation.annotation_id.as_bytes().as_slice()],
            )
            .map_err(StorageError::Database)?;
        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO annotation_context_tags (annotation_id, tag)
                     VALUES (?1, ?2)",
                )
                .map_err(StorageError::Database)?;
            for tag in &annotation.context_tags {
                statement
                    .execute(params![
                        annotation.annotation_id.as_bytes().as_slice(),
                        tag.as_str()
                    ])
                    .map_err(StorageError::Database)?;
            }
        }
        transaction.commit().map_err(StorageError::Database)
    }

    pub fn annotations(&self) -> Result<Vec<AnnotationRecord>, StorageError> {
        let mut tags = BTreeMap::<Uuid, Vec<ContextTag>>::new();
        {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT annotation_id, tag
                     FROM annotation_context_tags
                     ORDER BY annotation_id, tag",
                )
                .map_err(StorageError::Database)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(StorageError::Database)?;
            for row in rows {
                let (annotation_id, wire_tag) = row.map_err(StorageError::Database)?;
                let annotation_id = uuid_from_blob(&annotation_id)?;
                let tag = ContextTag::from_wire(&wire_tag)
                    .ok_or_else(|| StorageError::InvalidStoredContextTag(wire_tag))?;
                tags.entry(annotation_id).or_default().push(tag);
            }
        }

        let mut statement = self
            .connection
            .prepare(
                "SELECT annotation_id, created_at_ms, time_zone, local_date,
                        start_minute, end_minute, text
                 FROM annotations
                 ORDER BY local_date, start_minute, created_at_ms",
            )
            .map_err(StorageError::Database)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(StorageError::Database)?;

        let mut annotations = Vec::new();
        for row in rows {
            let (id, created_at_ms, time_zone, local_date, start_minute, end_minute, text) =
                row.map_err(StorageError::Database)?;
            let annotation_id = uuid_from_blob(&id)?;
            annotations.push(AnnotationRecord {
                annotation_id,
                created_at: DateTime::<Utc>::from_timestamp_millis(created_at_ms).ok_or(
                    StorageError::InvalidStoredTimestamp {
                        timestamp_ms: created_at_ms,
                    },
                )?,
                time_zone,
                local_date,
                start_minute: optional_u16(start_minute)?,
                end_minute: optional_u16(end_minute)?,
                text,
                context_tags: tags.remove(&annotation_id).unwrap_or_default(),
            });
        }
        Ok(annotations)
    }

    pub fn delete_annotation(&mut self, annotation_id: Uuid) -> Result<bool, StorageError> {
        let deleted = self
            .connection
            .execute(
                "DELETE FROM annotations WHERE annotation_id = ?1",
                [annotation_id.as_bytes().as_slice()],
            )
            .map_err(StorageError::Database)?;
        Ok(deleted > 0)
    }

    pub fn collection_settings(
        &self,
        at: DateTime<Utc>,
    ) -> Result<Vec<SignalCollectionSetting>, StorageError> {
        SignalId::ALL
            .into_iter()
            .map(|signal| {
                let transition = self.latest_signal_transition(signal, at)?;
                Ok(SignalCollectionSetting {
                    signal,
                    enabled: transition.is_some_and(|(_, enabled)| enabled),
                    changed_at: transition.map(|(changed_at, _)| changed_at),
                })
            })
            .collect()
    }

    pub fn source_activity_timestamps(&self) -> Result<SourceActivityTimestamps, StorageError> {
        let mut timestamps = SourceActivityTimestamps::default();

        {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT m.signal_id, MAX(b.received_at_ms)
                     FROM aggregate_metrics m
                     JOIN aggregate_batches b ON b.batch_id = m.batch_id
                     GROUP BY m.signal_id",
                )
                .map_err(StorageError::Database)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(StorageError::Database)?;

            for row in rows {
                let (signal_id, timestamp_ms) = row.map_err(StorageError::Database)?;
                let signal = SignalId::from_wire(&signal_id)
                    .ok_or_else(|| StorageError::InvalidStoredSignal(signal_id))?;
                let timestamp = DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
                    .ok_or(StorageError::InvalidStoredTimestamp { timestamp_ms })?;

                match signal.source_type() {
                    SourceType::Browser => {
                        timestamps.browser = Some(latest_timestamp(timestamps.browser, timestamp));
                    }
                    SourceType::Os => {
                        timestamps.os = Some(latest_timestamp(timestamps.os, timestamp));
                    }
                    SourceType::CheckIn => unreachable!("aggregate signals cannot be check-ins"),
                }
            }
        }

        let check_in_timestamp_ms = self
            .connection
            .query_row("SELECT MAX(created_at_ms) FROM check_ins", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .map_err(StorageError::Database)?;
        timestamps.check_in = check_in_timestamp_ms
            .map(|timestamp_ms| {
                DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
                    .ok_or(StorageError::InvalidStoredTimestamp { timestamp_ms })
            })
            .transpose()?;

        Ok(timestamps)
    }

    pub fn set_signal_collection(
        &mut self,
        signal: SignalId,
        enabled: bool,
        effective_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        if self
            .latest_signal_transition(signal, effective_at)?
            .is_some_and(|(_, current)| current == enabled)
        {
            return Ok(());
        }

        self.connection
            .execute(
                "INSERT INTO signal_collection_transitions (
                    signal_id,
                    effective_at_ms,
                    enabled,
                    created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(signal_id, effective_at_ms)
                 DO UPDATE SET
                    enabled = excluded.enabled,
                    created_at_ms = excluded.created_at_ms",
                params![
                    signal.as_str(),
                    effective_at.timestamp_millis(),
                    i64::from(enabled),
                    Utc::now().timestamp_millis(),
                ],
            )
            .map_err(StorageError::Database)?;
        Ok(())
    }

    pub fn signal_record_summary(
        &self,
        signal: SignalId,
    ) -> Result<SignalRecordSummary, StorageError> {
        signal_record_summary_on(&self.connection, signal)
    }

    pub fn delete_signal_records(
        &mut self,
        signal: SignalId,
    ) -> Result<SignalRecordSummary, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::Database)?;
        let deleted = signal_record_summary_on(&transaction, signal)?;

        transaction
            .execute(
                "DELETE FROM aggregate_metrics WHERE signal_id = ?1",
                [signal.as_str()],
            )
            .map_err(StorageError::Database)?;
        transaction
            .execute(
                "DELETE FROM aggregate_batches
                 WHERE NOT EXISTS (
                   SELECT 1
                   FROM aggregate_metrics m
                   WHERE m.batch_id = aggregate_batches.batch_id
                 )",
                [],
            )
            .map_err(StorageError::Database)?;
        transaction.commit().map_err(StorageError::Database)?;
        Ok(deleted)
    }

    pub fn signal_enabled_for_period(
        &self,
        signal: SignalId,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<bool, StorageError> {
        let enabled_at_start = self
            .latest_signal_transition(signal, start)?
            .is_some_and(|(_, enabled)| enabled);
        if !enabled_at_start {
            return Ok(false);
        }

        let transition_count = self
            .connection
            .query_row(
                "SELECT COUNT(*)
                 FROM signal_collection_transitions
                 WHERE signal_id = ?1
                   AND effective_at_ms > ?2
                   AND effective_at_ms < ?3",
                params![
                    signal.as_str(),
                    start.timestamp_millis(),
                    end.timestamp_millis()
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(StorageError::Database)?;
        Ok(transition_count == 0)
    }

    pub fn aggregate_batch_count(&self) -> Result<u64, StorageError> {
        let count = self
            .connection
            .query_row("SELECT COUNT(*) FROM aggregate_batches", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(StorageError::Database)?;
        u64::try_from(count).map_err(|_| StorageError::InvalidStoredCount)
    }

    pub fn metric_count(&self) -> Result<u64, StorageError> {
        let count = self
            .connection
            .query_row("SELECT COUNT(*) FROM aggregate_metrics", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(StorageError::Database)?;
        u64::try_from(count).map_err(|_| StorageError::InvalidStoredCount)
    }

    pub fn check_in_count(&self) -> Result<u64, StorageError> {
        let count = self
            .connection
            .query_row("SELECT COUNT(*) FROM check_ins", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(StorageError::Database)?;
        u64::try_from(count).map_err(|_| StorageError::InvalidStoredCount)
    }

    pub fn check_in_context_tag_count(&self) -> Result<u64, StorageError> {
        let count = self
            .connection
            .query_row("SELECT COUNT(*) FROM check_in_context_tags", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(StorageError::Database)?;
        u64::try_from(count).map_err(|_| StorageError::InvalidStoredCount)
    }

    pub fn annotation_count(&self) -> Result<u64, StorageError> {
        table_count(&self.connection, "annotations")
    }

    pub fn annotation_context_tag_count(&self) -> Result<u64, StorageError> {
        table_count(&self.connection, "annotation_context_tags")
    }

    pub fn daily_browser_features(&self) -> Result<Vec<DailyBrowserFeatures>, StorageError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT b.batch_id, b.period_start_ms, b.time_zone, m.signal_id, m.value
                 FROM aggregate_batches b
                 JOIN aggregate_metrics m ON m.batch_id = b.batch_id
                 ORDER BY b.period_start_ms, m.signal_id",
            )
            .map_err(StorageError::Database)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .map_err(StorageError::Database)?;

        let mut days = BTreeMap::<String, DailyBrowserAccumulator>::new();
        for row in rows {
            let (batch_id, period_start_ms, time_zone, signal_id, value) =
                row.map_err(StorageError::Database)?;
            let timestamp = DateTime::<Utc>::from_timestamp_millis(period_start_ms).ok_or(
                StorageError::InvalidStoredTimestamp {
                    timestamp_ms: period_start_ms,
                },
            )?;
            let parsed_time_zone = time_zone
                .parse::<chrono_tz::Tz>()
                .map_err(|_| StorageError::InvalidStoredTimeZone(time_zone))?;
            let signal = SignalId::from_wire(&signal_id)
                .ok_or_else(|| StorageError::InvalidStoredSignal(signal_id))?;
            let local_date = timestamp
                .with_timezone(&parsed_time_zone)
                .date_naive()
                .to_string();

            days.entry(local_date)
                .or_default()
                .record(batch_id, signal, value);
        }

        days.into_iter()
            .map(|(local_date, accumulator)| accumulator.into_daily(local_date))
            .collect()
    }

    pub fn daily_os_features(&self) -> Result<Vec<DailyOsFeatures>, StorageError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT b.batch_id, b.period_start_ms, b.time_zone, m.signal_id, m.value
                 FROM aggregate_batches b
                 JOIN aggregate_metrics m ON m.batch_id = b.batch_id
                 WHERE m.signal_id LIKE 'os.%'
                 ORDER BY b.period_start_ms, m.signal_id",
            )
            .map_err(StorageError::Database)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .map_err(StorageError::Database)?;

        let mut days = BTreeMap::<String, DailyOsAccumulator>::new();
        for row in rows {
            let (batch_id, period_start_ms, time_zone, signal_id, value) =
                row.map_err(StorageError::Database)?;
            let timestamp = DateTime::<Utc>::from_timestamp_millis(period_start_ms).ok_or(
                StorageError::InvalidStoredTimestamp {
                    timestamp_ms: period_start_ms,
                },
            )?;
            let parsed_time_zone = time_zone
                .parse::<chrono_tz::Tz>()
                .map_err(|_| StorageError::InvalidStoredTimeZone(time_zone))?;
            let signal = SignalId::from_wire(&signal_id)
                .ok_or_else(|| StorageError::InvalidStoredSignal(signal_id))?;
            let local_date = timestamp
                .with_timezone(&parsed_time_zone)
                .date_naive()
                .to_string();

            days.entry(local_date)
                .or_default()
                .record(batch_id, signal, value);
        }

        days.into_iter()
            .map(|(local_date, accumulator)| accumulator.into_daily(local_date))
            .collect()
    }

    pub fn daily_check_in_features(&self) -> Result<Vec<DailyCheckInFeatures>, StorageError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT
                    local_date,
                    AVG(CAST(sleep_minutes AS REAL)),
                    AVG(CAST(mood AS REAL)),
                    AVG(CAST(energy AS REAL)),
                    AVG(CAST(irritability AS REAL)),
                    AVG(CAST(concentration AS REAL)),
                    AVG(CAST(impulsivity AS REAL)),
                    COUNT(*)
                 FROM check_ins
                 GROUP BY local_date
                 ORDER BY local_date",
            )
            .map_err(StorageError::Database)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, Option<f64>>(5)?,
                    row.get::<_, Option<f64>>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            })
            .map_err(StorageError::Database)?;

        let mut context_tags = BTreeMap::<String, BTreeSet<ContextTag>>::new();
        {
            let mut tag_statement = self
                .connection
                .prepare(
                    "SELECT c.local_date, t.tag
                     FROM check_ins c
                     JOIN check_in_context_tags t ON t.check_in_id = c.check_in_id
                     ORDER BY c.local_date, t.tag",
                )
                .map_err(StorageError::Database)?;
            let tag_rows = tag_statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(StorageError::Database)?;
            for row in tag_rows {
                let (local_date, wire_tag) = row.map_err(StorageError::Database)?;
                let tag = ContextTag::from_wire(&wire_tag)
                    .ok_or_else(|| StorageError::InvalidStoredContextTag(wire_tag))?;
                context_tags.entry(local_date).or_default().insert(tag);
            }
        }

        let mut features = Vec::new();
        for row in rows {
            let (
                local_date,
                sleep_minutes,
                mood,
                energy,
                irritability,
                concentration,
                impulsivity,
                check_in_count,
            ) = row.map_err(StorageError::Database)?;
            let tags = context_tags
                .remove(&local_date)
                .map_or_else(Vec::new, |tags| tags.into_iter().collect());
            features.push(DailyCheckInFeatures {
                local_date,
                sleep_minutes,
                mood,
                energy,
                irritability,
                concentration,
                impulsivity,
                check_in_count: u64::try_from(check_in_count)
                    .map_err(|_| StorageError::InvalidStoredCount)?,
                context_tags: tags,
            });
        }

        Ok(features)
    }

    pub fn schema_version(&self) -> Result<i64, StorageError> {
        self.connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(StorageError::Database)
    }

    pub fn clear_all(&mut self) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::Database)?;
        transaction
            .execute_batch(
                "DELETE FROM aggregate_metrics;
                 DELETE FROM aggregate_batches;
                 DELETE FROM source_sequences;
                 DELETE FROM annotation_context_tags;
                 DELETE FROM annotations;
                 DELETE FROM check_in_context_tags;
                 DELETE FROM check_ins;",
            )
            .map_err(StorageError::Database)?;
        transaction.commit().map_err(StorageError::Database)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn create_encrypted_backup(
        &self,
        backup_path: impl AsRef<Path>,
        created_at: DateTime<Utc>,
    ) -> Result<EncryptedBackup, StorageError> {
        let backup_path = backup_path.as_ref();
        validate_new_backup_path(backup_path)?;
        let recovery_key = DatabaseKey::generate()?;
        let recovery_secret = recovery_secret(&recovery_key);
        let partial_path = backup_partial_path(backup_path)?;
        ensure_private_backup_file(&partial_path)?;

        let result = (|| {
            attach_encrypted_database(&self.connection, &partial_path, "backup", &recovery_key)?;
            let export_result = (|| {
                self.connection
                    .query_row("SELECT sqlcipher_export('backup')", [], |_| Ok(()))
                    .map_err(StorageError::Database)?;
                self.connection
                    .execute_batch(
                        "CREATE TABLE backup.mindcanary_backup_metadata (
                            format_version INTEGER NOT NULL,
                            schema_version INTEGER NOT NULL,
                            created_at_ms INTEGER NOT NULL
                         ) STRICT;",
                    )
                    .map_err(StorageError::Database)?;
                self.connection
                    .execute(
                        "INSERT INTO backup.mindcanary_backup_metadata (
                            format_version, schema_version, created_at_ms
                         ) VALUES (?1, ?2, ?3)",
                        params![
                            BACKUP_FORMAT_VERSION,
                            CURRENT_SCHEMA_VERSION,
                            created_at.timestamp_millis()
                        ],
                    )
                    .map_err(StorageError::Database)?;
                self.connection
                    .pragma_update(Some("backup"), "user_version", CURRENT_SCHEMA_VERSION)
                    .map_err(StorageError::Database)
            })();
            let detach_result = self
                .connection
                .execute_batch("DETACH DATABASE backup")
                .map_err(StorageError::Database);
            export_result.and(detach_result)?;

            verify_encrypted_backup(&partial_path, &recovery_key)?;
            fs::rename(&partial_path, backup_path).map_err(StorageError::Io)?;
            Ok(EncryptedBackup {
                path: backup_path.to_owned(),
                created_at,
                format_version: BACKUP_FORMAT_VERSION,
                schema_version: CURRENT_SCHEMA_VERSION,
                recovery_secret,
            })
        })();

        if result.is_err() {
            let _ = fs::remove_file(&partial_path);
        }
        result
    }

    pub fn verify_encrypted_backup(
        backup_path: impl AsRef<Path>,
        recovery_secret: &str,
    ) -> Result<EncryptedBackupMetadata, StorageError> {
        let key = key_from_recovery_secret(recovery_secret)?;
        verify_encrypted_backup(backup_path.as_ref(), &key)
    }

    pub fn restore_encrypted_backup(
        &mut self,
        backup_path: impl AsRef<Path>,
        recovery_secret: &str,
    ) -> Result<BackupRestoreReport, StorageError> {
        if !self.canonical_records_are_empty()? {
            return Err(StorageError::RestoreRequiresEmptyRecords);
        }

        let backup_path = backup_path.as_ref();
        let recovery_key = key_from_recovery_secret(recovery_secret)?;
        let metadata = verify_encrypted_backup(backup_path, &recovery_key)?;
        if metadata.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(StorageError::UnsupportedBackupSchema {
                found: metadata.schema_version,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }

        attach_encrypted_database(&self.connection, backup_path, "backup", &recovery_key)?;
        let restore_result = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(StorageError::Database)?;
            transaction
                .execute_batch(
                    "DELETE FROM aggregate_metrics;
                     DELETE FROM aggregate_batches;
                     DELETE FROM source_sequences;
                     DELETE FROM annotation_context_tags;
                     DELETE FROM annotations;
                     DELETE FROM check_in_context_tags;
                     DELETE FROM check_ins;
                     DELETE FROM signal_collection_transitions;

                     INSERT INTO aggregate_batches SELECT * FROM backup.aggregate_batches;
                     INSERT INTO aggregate_metrics SELECT * FROM backup.aggregate_metrics;
                     INSERT INTO source_sequences SELECT * FROM backup.source_sequences;
                     INSERT INTO check_ins SELECT * FROM backup.check_ins;
                     INSERT INTO check_in_context_tags SELECT * FROM backup.check_in_context_tags;
                     INSERT INTO signal_collection_transitions
                       SELECT * FROM backup.signal_collection_transitions;
                     INSERT INTO annotations SELECT * FROM backup.annotations;
                     INSERT INTO annotation_context_tags
                       SELECT * FROM backup.annotation_context_tags;",
                )
                .map_err(StorageError::Database)?;
            transaction.commit().map_err(StorageError::Database)
        })();
        let detach_result = self
            .connection
            .execute_batch("DETACH DATABASE backup")
            .map_err(StorageError::Database);
        restore_result.and(detach_result)?;

        Ok(BackupRestoreReport {
            aggregate_batch_count: self.aggregate_batch_count()?,
            aggregate_metric_count: self.metric_count()?,
            check_in_count: self.check_in_count()?,
            annotation_count: self.annotation_count()?,
        })
    }

    fn canonical_records_are_empty(&self) -> Result<bool, StorageError> {
        Ok(self.aggregate_batch_count()? == 0
            && self.metric_count()? == 0
            && self.check_in_count()? == 0
            && self.annotation_count()? == 0)
    }

    fn latest_signal_transition(
        &self,
        signal: SignalId,
        at: DateTime<Utc>,
    ) -> Result<Option<(DateTime<Utc>, bool)>, StorageError> {
        let transition = self
            .connection
            .query_row(
                "SELECT effective_at_ms, enabled
                 FROM signal_collection_transitions
                 WHERE signal_id = ?1 AND effective_at_ms <= ?2
                 ORDER BY effective_at_ms DESC
                 LIMIT 1",
                params![signal.as_str(), at.timestamp_millis()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(StorageError::Database)?;

        transition
            .map(|(timestamp_ms, enabled)| {
                let changed_at = DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
                    .ok_or(StorageError::InvalidStoredTimestamp { timestamp_ms })?;
                Ok((changed_at, enabled != 0))
            })
            .transpose()
    }
}

fn signal_record_summary_on(
    connection: &Connection,
    signal: SignalId,
) -> Result<SignalRecordSummary, StorageError> {
    let (metric_record_count, affected_batch_count) = connection
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT batch_id)
             FROM aggregate_metrics
             WHERE signal_id = ?1",
            [signal.as_str()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(StorageError::Database)?;

    Ok(SignalRecordSummary {
        metric_record_count: u64::try_from(metric_record_count)
            .map_err(|_| StorageError::InvalidStoredCount)?,
        affected_batch_count: u64::try_from(affected_batch_count)
            .map_err(|_| StorageError::InvalidStoredCount)?,
    })
}

fn optional_bool_to_i64(value: Option<bool>) -> Option<i64> {
    value.map(i64::from)
}

fn table_count(connection: &Connection, table: &str) -> Result<u64, StorageError> {
    let query = match table {
        "annotations" => "SELECT COUNT(*) FROM annotations",
        "annotation_context_tags" => "SELECT COUNT(*) FROM annotation_context_tags",
        _ => unreachable!("table count is limited to annotation tables"),
    };
    let count = connection
        .query_row(query, [], |row| row.get::<_, i64>(0))
        .map_err(StorageError::Database)?;
    u64::try_from(count).map_err(|_| StorageError::InvalidStoredCount)
}

fn uuid_from_blob(bytes: &[u8]) -> Result<Uuid, StorageError> {
    Uuid::from_slice(bytes).map_err(|_| StorageError::InvalidStoredUuid)
}

fn optional_u16(value: Option<i64>) -> Result<Option<u16>, StorageError> {
    value
        .map(|value| u16::try_from(value).map_err(|_| StorageError::InvalidStoredMinute))
        .transpose()
}

#[derive(Debug, Default)]
struct DailyBrowserAccumulator {
    batch_ids: BTreeSet<Vec<u8>>,
    open_tab_count_mean: MeanAccumulator,
    open_tab_count_max: Option<f64>,
    tab_switch_count: Option<f64>,
    retained_across_day_count: Option<f64>,
    continuous_scrolling_seconds: Option<f64>,
    active_seconds: Option<f64>,
    idle_seconds: Option<f64>,
}

impl DailyBrowserAccumulator {
    fn record(&mut self, batch_id: Vec<u8>, signal: SignalId, value: f64) {
        self.batch_ids.insert(batch_id);
        match signal {
            SignalId::BrowserOpenTabCountMean => self.open_tab_count_mean.push(value),
            SignalId::BrowserOpenTabCountMax => {
                self.open_tab_count_max = Some(
                    self.open_tab_count_max
                        .map_or(value, |current| current.max(value)),
                );
            }
            SignalId::BrowserTabSwitchCount => add_sum(&mut self.tab_switch_count, value),
            SignalId::BrowserRetainedAcrossDayCount => {
                add_sum(&mut self.retained_across_day_count, value);
            }
            SignalId::BrowserContinuousScrollingSeconds => {
                add_sum(&mut self.continuous_scrolling_seconds, value);
            }
            SignalId::BrowserActiveSeconds => add_sum(&mut self.active_seconds, value),
            SignalId::BrowserIdleSeconds => add_sum(&mut self.idle_seconds, value),
            SignalId::BrowserOpenTabCountMin
            | SignalId::BrowserTabOpenCount
            | SignalId::BrowserTabCloseCount
            | SignalId::BrowserWindowCountMax
            | SignalId::OsActiveSeconds
            | SignalId::OsIdleSeconds
            | SignalId::OsLockCount
            | SignalId::OsUnlockCount
            | SignalId::OsSuspendCount
            | SignalId::OsResumeCount => {}
        }
    }

    fn into_daily(self, local_date: String) -> Result<DailyBrowserFeatures, StorageError> {
        Ok(DailyBrowserFeatures {
            local_date,
            open_tab_count_mean: self.open_tab_count_mean.finish(),
            open_tab_count_max: self.open_tab_count_max,
            tab_switch_count: self.tab_switch_count,
            retained_across_day_count: self.retained_across_day_count,
            continuous_scrolling_seconds: self.continuous_scrolling_seconds,
            active_seconds: self.active_seconds,
            idle_seconds: self.idle_seconds,
            aggregate_bucket_count: u64::try_from(self.batch_ids.len())
                .map_err(|_| StorageError::InvalidStoredCount)?,
        })
    }
}

#[derive(Debug, Default)]
struct DailyOsAccumulator {
    batch_ids: BTreeSet<Vec<u8>>,
    active_seconds: Option<f64>,
    idle_seconds: Option<f64>,
    lock_count: Option<f64>,
    unlock_count: Option<f64>,
    suspend_count: Option<f64>,
    resume_count: Option<f64>,
}

impl DailyOsAccumulator {
    fn record(&mut self, batch_id: Vec<u8>, signal: SignalId, value: f64) {
        self.batch_ids.insert(batch_id);
        match signal {
            SignalId::OsActiveSeconds => add_sum(&mut self.active_seconds, value),
            SignalId::OsIdleSeconds => add_sum(&mut self.idle_seconds, value),
            SignalId::OsLockCount => add_sum(&mut self.lock_count, value),
            SignalId::OsUnlockCount => add_sum(&mut self.unlock_count, value),
            SignalId::OsSuspendCount => add_sum(&mut self.suspend_count, value),
            SignalId::OsResumeCount => add_sum(&mut self.resume_count, value),
            SignalId::BrowserTabSwitchCount
            | SignalId::BrowserOpenTabCountMin
            | SignalId::BrowserOpenTabCountMax
            | SignalId::BrowserOpenTabCountMean
            | SignalId::BrowserTabOpenCount
            | SignalId::BrowserTabCloseCount
            | SignalId::BrowserWindowCountMax
            | SignalId::BrowserActiveSeconds
            | SignalId::BrowserIdleSeconds
            | SignalId::BrowserRetainedAcrossDayCount
            | SignalId::BrowserContinuousScrollingSeconds => {}
        }
    }

    fn into_daily(self, local_date: String) -> Result<DailyOsFeatures, StorageError> {
        Ok(DailyOsFeatures {
            local_date,
            active_seconds: self.active_seconds,
            idle_seconds: self.idle_seconds,
            lock_count: self.lock_count,
            unlock_count: self.unlock_count,
            suspend_count: self.suspend_count,
            resume_count: self.resume_count,
            aggregate_bucket_count: u64::try_from(self.batch_ids.len())
                .map_err(|_| StorageError::InvalidStoredCount)?,
        })
    }
}

#[derive(Debug, Default)]
struct MeanAccumulator {
    sum: f64,
    count: f64,
}

impl MeanAccumulator {
    fn push(&mut self, value: f64) {
        self.sum += value;
        self.count += 1.0;
    }

    fn finish(self) -> Option<f64> {
        if self.count == 0.0 {
            None
        } else {
            Some(self.sum / self.count)
        }
    }
}

fn add_sum(slot: &mut Option<f64>, value: f64) {
    *slot = Some(slot.unwrap_or_default() + value);
}

fn prepare_database_parent(path: &Path) -> Result<(), StorageError> {
    let parent = path.parent().ok_or(StorageError::MissingDatabaseParent)?;
    std::fs::create_dir_all(parent).map_err(StorageError::Io)?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
        .map_err(StorageError::Io)
}

fn ensure_private_database_file(path: &Path) -> Result<(), StorageError> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(StorageError::Io)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(StorageError::Io)
}

fn validate_new_backup_path(path: &Path) -> Result<(), StorageError> {
    if !path.is_absolute() {
        return Err(StorageError::BackupPathNotAbsolute);
    }
    let parent = path.parent().ok_or(StorageError::MissingDatabaseParent)?;
    if !parent.is_dir() {
        return Err(StorageError::BackupParentMissing);
    }
    if path.exists() {
        return Err(StorageError::BackupAlreadyExists);
    }
    path.file_name()
        .ok_or(StorageError::MissingDatabaseFilename)?;
    Ok(())
}

fn backup_partial_path(path: &Path) -> Result<PathBuf, StorageError> {
    let filename = path
        .file_name()
        .ok_or(StorageError::MissingDatabaseFilename)?
        .to_string_lossy();
    Ok(path.with_file_name(format!("{filename}.partial-{}", Uuid::now_v7())))
}

fn ensure_private_backup_file(path: &Path) -> Result<(), StorageError> {
    let file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(StorageError::Io)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(StorageError::Io)
}

fn attach_encrypted_database(
    connection: &Connection,
    path: &Path,
    schema: &str,
    key: &DatabaseKey,
) -> Result<(), StorageError> {
    if schema != "backup" {
        return Err(StorageError::InvalidBackupSchemaName);
    }
    let key_literal = key.sqlcipher_literal();
    let statement = format!(
        "ATTACH DATABASE ?1 AS {schema} KEY \"{}\"",
        key_literal.as_str()
    );
    connection
        .execute(&statement, [path.to_string_lossy().as_ref()])
        .map_err(StorageError::Database)?;
    Ok(())
}

fn recovery_secret(key: &DatabaseKey) -> String {
    use fmt::Write as _;

    let mut secret = String::with_capacity(DATABASE_KEY_BYTES * 2 + 15);
    for (index, byte) in key.as_bytes().iter().enumerate() {
        if index > 0 && index % 2 == 0 {
            secret.push('-');
        }
        let _ = write!(secret, "{byte:02X}");
    }
    secret
}

fn key_from_recovery_secret(secret: &str) -> Result<DatabaseKey, StorageError> {
    let compact = zeroize::Zeroizing::new(
        secret
            .chars()
            .filter(|character| *character != '-' && !character.is_ascii_whitespace())
            .collect::<String>(),
    );
    if compact.len() != DATABASE_KEY_BYTES * 2
        || !compact
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(StorageError::InvalidRecoverySecret);
    }

    let mut bytes = [0_u8; DATABASE_KEY_BYTES];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&compact[offset..offset + 2], 16)
            .map_err(|_| StorageError::InvalidRecoverySecret)?;
    }
    Ok(DatabaseKey::from_bytes(bytes))
}

fn verify_encrypted_backup(
    path: &Path,
    key: &DatabaseKey,
) -> Result<EncryptedBackupMetadata, StorageError> {
    if !path.is_absolute() {
        return Err(StorageError::BackupPathNotAbsolute);
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(StorageError::Database)?;
    apply_key(&connection, key)?;
    verify_sqlcipher(&connection).map_err(|error| match error {
        StorageError::InvalidDatabaseKey(source) => StorageError::InvalidBackupKey(source),
        other => other,
    })?;

    let integrity = connection
        .pragma_query_value(None, "integrity_check", |row| row.get::<_, String>(0))
        .map_err(StorageError::Database)?;
    if integrity != "ok" {
        return Err(StorageError::BackupIntegrityFailed);
    }

    let (format_version, schema_version, created_at_ms) = connection
        .query_row(
            "SELECT format_version, schema_version, created_at_ms
             FROM mindcanary_backup_metadata",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .map_err(|_| StorageError::InvalidBackupFormat)?;
    if format_version != BACKUP_FORMAT_VERSION {
        return Err(StorageError::UnsupportedBackupFormat {
            found: format_version,
            supported: BACKUP_FORMAT_VERSION,
        });
    }
    let created_at = DateTime::<Utc>::from_timestamp_millis(created_at_ms)
        .ok_or(StorageError::InvalidBackupTimestamp)?;
    Ok(EncryptedBackupMetadata {
        created_at,
        format_version,
        schema_version,
    })
}

fn apply_key(connection: &Connection, key: &DatabaseKey) -> Result<(), StorageError> {
    let literal = key.sqlcipher_literal();
    connection
        .pragma_update(None, "key", literal.as_str())
        .map_err(StorageError::Database)?;
    connection
        .pragma_update(None, "cipher_memory_security", "ON")
        .map_err(StorageError::Database)
}

fn verify_sqlcipher(connection: &Connection) -> Result<(), StorageError> {
    let version = connection
        .pragma_query_value(None, "cipher_version", |row| row.get::<_, String>(0))
        .optional()
        .map_err(StorageError::Database)?
        .filter(|version| !version.is_empty())
        .ok_or(StorageError::SqlCipherUnavailable)?;

    if version.is_empty() {
        return Err(StorageError::SqlCipherUnavailable);
    }

    connection
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(StorageError::InvalidDatabaseKey)?;
    Ok(())
}

fn configure_connection(connection: &Connection) -> Result<(), StorageError> {
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(StorageError::Database)?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(StorageError::Database)?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(StorageError::Database)?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(StorageError::Database)
}

fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let mut current_version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(StorageError::Database)?;

    if current_version > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchemaVersion {
            found: current_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }

    if current_version < 1 {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::Database)?;
        transaction
            .execute_batch(
                "CREATE TABLE aggregate_batches (
                    batch_id BLOB PRIMARY KEY NOT NULL CHECK(length(batch_id) = 16),
                    source_instance_id BLOB NOT NULL CHECK(length(source_instance_id) = 16),
                    source_sequence INTEGER NOT NULL CHECK(source_sequence >= 0),
                    period_start_ms INTEGER NOT NULL,
                    period_end_ms INTEGER NOT NULL CHECK(period_end_ms > period_start_ms),
                    time_zone TEXT NOT NULL CHECK(length(time_zone) BETWEEN 1 AND 64),
                    received_at_ms INTEGER NOT NULL,
                    UNIQUE(source_instance_id, source_sequence)
                 ) STRICT;

                 CREATE TABLE aggregate_metrics (
                    batch_id BLOB NOT NULL CHECK(length(batch_id) = 16),
                    signal_id TEXT NOT NULL CHECK(length(signal_id) BETWEEN 1 AND 64),
                    value REAL NOT NULL,
                    PRIMARY KEY(batch_id, signal_id),
                    FOREIGN KEY(batch_id) REFERENCES aggregate_batches(batch_id)
                      ON DELETE CASCADE
                 ) STRICT;

                 CREATE TABLE source_sequences (
                    source_instance_id BLOB PRIMARY KEY NOT NULL
                      CHECK(length(source_instance_id) = 16),
                    last_sequence INTEGER NOT NULL CHECK(last_sequence >= 0)
                 ) STRICT;

                 CREATE INDEX aggregate_batches_period_start_idx
                   ON aggregate_batches(period_start_ms);",
            )
            .map_err(StorageError::Database)?;
        transaction
            .pragma_update(None, "user_version", 1)
            .map_err(StorageError::Database)?;
        transaction.commit().map_err(StorageError::Database)?;
        current_version = 1;
    }

    if current_version < 2 {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StorageError::Database)?;
        transaction
            .execute_batch(
                "CREATE TABLE check_ins (
                    check_in_id BLOB PRIMARY KEY NOT NULL CHECK(length(check_in_id) = 16),
                    occurred_at_ms INTEGER NOT NULL,
                    time_zone TEXT NOT NULL CHECK(length(time_zone) BETWEEN 1 AND 64),
                    local_date TEXT NOT NULL CHECK(length(local_date) = 10),
                    sleep_minutes INTEGER CHECK(sleep_minutes BETWEEN 0 AND 1440),
                    perceived_sleep_need INTEGER CHECK(perceived_sleep_need BETWEEN 1 AND 7),
                    mood INTEGER CHECK(mood BETWEEN 1 AND 7),
                    energy INTEGER CHECK(energy BETWEEN 1 AND 7),
                    irritability INTEGER CHECK(irritability BETWEEN 1 AND 7),
                    concentration INTEGER CHECK(concentration BETWEEN 1 AND 7),
                    impulsivity INTEGER CHECK(impulsivity BETWEEN 1 AND 7),
                    medication_taken INTEGER CHECK(medication_taken IN (0, 1)),
                    substance_use INTEGER CHECK(substance_use IN (0, 1)),
                    created_at_ms INTEGER NOT NULL
                 ) STRICT;

                 CREATE TABLE check_in_context_tags (
                    check_in_id BLOB NOT NULL CHECK(length(check_in_id) = 16),
                    tag TEXT NOT NULL CHECK(length(tag) BETWEEN 1 AND 64),
                    PRIMARY KEY(check_in_id, tag),
                    FOREIGN KEY(check_in_id) REFERENCES check_ins(check_in_id)
                      ON DELETE CASCADE
                 ) STRICT;

                 CREATE INDEX check_ins_local_date_idx
                   ON check_ins(local_date);
                 CREATE INDEX check_ins_occurred_at_idx
                   ON check_ins(occurred_at_ms);",
            )
            .map_err(StorageError::Database)?;
        transaction
            .pragma_update(None, "user_version", 2)
            .map_err(StorageError::Database)?;
        transaction.commit().map_err(StorageError::Database)?;
        current_version = 2;
    }

    if current_version < 3 {
        migrate_signal_collection_transitions(connection)?;
        current_version = 3;
    }

    if current_version < 4 {
        migrate_annotations(connection)?;
    }

    Ok(())
}

fn migrate_signal_collection_transitions(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(StorageError::Database)?;
    transaction
        .execute_batch(
            "CREATE TABLE signal_collection_transitions (
                signal_id TEXT NOT NULL CHECK(length(signal_id) BETWEEN 1 AND 64),
                effective_at_ms INTEGER NOT NULL,
                enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
                created_at_ms INTEGER NOT NULL,
                PRIMARY KEY(signal_id, effective_at_ms)
             ) STRICT;

             CREATE INDEX signal_collection_transition_lookup_idx
               ON signal_collection_transitions(signal_id, effective_at_ms DESC);",
        )
        .map_err(StorageError::Database)?;
    transaction
        .pragma_update(None, "user_version", 3)
        .map_err(StorageError::Database)?;
    transaction.commit().map_err(StorageError::Database)
}

fn migrate_annotations(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(StorageError::Database)?;
    transaction
        .execute_batch(
            "CREATE TABLE annotations (
                annotation_id BLOB PRIMARY KEY NOT NULL CHECK(length(annotation_id) = 16),
                created_at_ms INTEGER NOT NULL,
                time_zone TEXT NOT NULL CHECK(length(time_zone) BETWEEN 1 AND 64),
                local_date TEXT NOT NULL CHECK(length(local_date) = 10),
                start_minute INTEGER CHECK(start_minute BETWEEN 0 AND 1439),
                end_minute INTEGER CHECK(end_minute BETWEEN 1 AND 1440),
                text TEXT NOT NULL CHECK(length(text) BETWEEN 1 AND 1000),
                CHECK((start_minute IS NULL AND end_minute IS NULL)
                   OR (start_minute IS NOT NULL AND end_minute IS NOT NULL
                       AND start_minute < end_minute))
             ) STRICT;

             CREATE TABLE annotation_context_tags (
                annotation_id BLOB NOT NULL CHECK(length(annotation_id) = 16),
                tag TEXT NOT NULL CHECK(length(tag) BETWEEN 1 AND 64),
                PRIMARY KEY(annotation_id, tag),
                FOREIGN KEY(annotation_id) REFERENCES annotations(annotation_id)
                  ON DELETE CASCADE
             ) STRICT;

             CREATE INDEX annotations_local_date_idx
               ON annotations(local_date, start_minute, created_at_ms);",
        )
        .map_err(StorageError::Database)?;
    transaction
        .pragma_update(None, "user_version", 4)
        .map_err(StorageError::Database)?;
    transaction.commit().map_err(StorageError::Database)
}

pub fn default_database_path() -> PathBuf {
    let data_root = std::env::var_os("XDG_DATA_HOME").map_or_else(
        || {
            std::env::var_os("HOME").map_or_else(
                || std::env::temp_dir().join("mindcanary-data"),
                |home| PathBuf::from(home).join(".local/share"),
            )
        },
        PathBuf::from,
    );
    data_root.join("mindcanary").join("mindcanary.db")
}

pub fn destroy_local_profile(
    database_path: impl AsRef<Path>,
    provider: &dyn DatabaseKeyProvider,
) -> Result<LocalProfileDestroyReport, StorageError> {
    let database_path = database_path.as_ref();
    let mut removed_files = Vec::new();

    for path in sqlite_database_files(database_path)? {
        match fs::remove_file(&path) {
            Ok(()) => removed_files.push(path),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(StorageError::Io(error)),
        }
    }

    provider.delete()?;

    Ok(LocalProfileDestroyReport {
        database_path: database_path.to_owned(),
        removed_files,
    })
}

fn sqlite_database_files(database_path: &Path) -> Result<Vec<PathBuf>, StorageError> {
    let parent = database_path
        .parent()
        .ok_or(StorageError::MissingDatabaseParent)?;
    let filename = database_path
        .file_name()
        .ok_or(StorageError::MissingDatabaseFilename)?
        .to_string_lossy();

    Ok([
        database_path.to_owned(),
        parent.join(format!("{filename}-wal")),
        parent.join(format!("{filename}-shm")),
        parent.join(format!("{filename}-journal")),
    ]
    .into())
}

pub fn database_file_looks_encrypted(path: &Path) -> Result<bool, StorageError> {
    let mut header = [0_u8; 16];
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(StorageError::Io(error)),
    };
    std::io::Read::read_exact(&mut file, &mut header).map_err(StorageError::Io)?;
    Ok(&header != b"SQLite format 3\0")
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database key generation failed")]
    KeyGeneration,
    #[error("database key must contain exactly 32 bytes")]
    InvalidKeyLength,
    #[error("encrypted database exists but its OS-keyring key is missing")]
    MissingDatabaseKey,
    #[error("OS keyring operation failed")]
    Keyring(#[source] keyring::Error),
    #[error("database operation failed")]
    Database(#[source] rusqlite::Error),
    #[error("database could not be opened with the supplied key")]
    InvalidDatabaseKey(#[source] rusqlite::Error),
    #[error("backup could not be opened with the supplied recovery secret")]
    InvalidBackupKey(#[source] rusqlite::Error),
    #[error("the linked SQLite library does not provide SQLCipher")]
    SqlCipherUnavailable,
    #[error("database path must have a parent directory")]
    MissingDatabaseParent,
    #[error("database path must include a file name")]
    MissingDatabaseFilename,
    #[error("backup path must be absolute")]
    BackupPathNotAbsolute,
    #[error("backup parent directory does not exist")]
    BackupParentMissing,
    #[error("backup destination already exists")]
    BackupAlreadyExists,
    #[error("recovery secret is invalid")]
    InvalidRecoverySecret,
    #[error("backup format metadata is missing or invalid")]
    InvalidBackupFormat,
    #[error("backup integrity check failed")]
    BackupIntegrityFailed,
    #[error("backup timestamp is invalid")]
    InvalidBackupTimestamp,
    #[error("backup format version {found} is unsupported; current version is {supported}")]
    UnsupportedBackupFormat { found: i64, supported: i64 },
    #[error("backup schema version {found} is unsupported; current version is {supported}")]
    UnsupportedBackupSchema { found: i64, supported: i64 },
    #[error("backup restore requires an empty local record set")]
    RestoreRequiresEmptyRecords,
    #[error("invalid internal backup schema name")]
    InvalidBackupSchemaName,
    #[error("database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchemaVersion { found: i64, supported: i64 },
    #[error("aggregate sequence conflicts with the stored source sequence")]
    SequenceConflict,
    #[error("aggregate sequence cannot be represented by the database")]
    SequenceOutOfRange,
    #[error("stored count was invalid")]
    InvalidStoredCount,
    #[error("stored UUID was invalid")]
    InvalidStoredUuid,
    #[error("stored annotation minute was invalid")]
    InvalidStoredMinute,
    #[error("stored aggregate timestamp {timestamp_ms}ms was invalid")]
    InvalidStoredTimestamp { timestamp_ms: i64 },
    #[error("stored aggregate time zone {0:?} was invalid")]
    InvalidStoredTimeZone(String),
    #[error("stored aggregate signal {0:?} was not recognized")]
    InvalidStoredSignal(String),
    #[error("stored local date {0:?} was invalid")]
    InvalidStoredLocalDate(String),
    #[error("stored check-in context tag {0:?} was not recognized")]
    InvalidStoredContextTag(String),
    #[error("filesystem operation failed")]
    Io(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::{TimeZone, Utc};
    use mindcanary_protocol::{
        AnnotationRecord, ContextTag, MAX_SAFE_SEQUENCE, Metric, ObservationPeriod, SignalId,
    };
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;

    #[derive(Debug, Default)]
    struct MemoryKeyProvider {
        key: Mutex<Option<DatabaseKey>>,
    }

    impl DatabaseKeyProvider for MemoryKeyProvider {
        fn load(&self) -> Result<Option<DatabaseKey>, StorageError> {
            Ok(self.key.lock().unwrap().clone())
        }

        fn store(&self, key: &DatabaseKey) -> Result<(), StorageError> {
            *self.key.lock().unwrap() = Some(key.clone());
            Ok(())
        }

        fn delete(&self) -> Result<(), StorageError> {
            *self.key.lock().unwrap() = None;
            Ok(())
        }
    }

    fn database_path(temp: &TempDir) -> PathBuf {
        temp.path().join("profile").join("mindcanary.db")
    }

    fn batch(source: Uuid, batch_id: Uuid, sequence: u64) -> AggregateBatch {
        batch_starting_at(
            source,
            batch_id,
            sequence,
            Utc.with_ymd_and_hms(2026, 6, 14, 12, 0, 0).unwrap(),
        )
    }

    fn batch_starting_at(
        source: Uuid,
        batch_id: Uuid,
        sequence: u64,
        start: chrono::DateTime<Utc>,
    ) -> AggregateBatch {
        AggregateBatch {
            batch_id,
            source_instance_id: source,
            sequence,
            period: ObservationPeriod {
                start,
                end: start + chrono::Duration::minutes(15),
                time_zone: "America/Sao_Paulo".to_owned(),
            },
            metrics: vec![
                Metric {
                    signal: SignalId::BrowserTabSwitchCount,
                    value: 18.0,
                },
                Metric {
                    signal: SignalId::BrowserOpenTabCountMax,
                    value: 31.0,
                },
            ],
        }
    }

    fn check_in(check_in_id: Uuid) -> CheckInRecord {
        CheckInRecord {
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
            context_tags: vec![ContextTag::Deadline, ContextTag::NewsCycle],
        }
    }

    fn annotation(annotation_id: Uuid) -> AnnotationRecord {
        AnnotationRecord {
            annotation_id,
            created_at: Utc.with_ymd_and_hms(2026, 6, 14, 18, 0, 0).unwrap(),
            time_zone: "America/Sao_Paulo".to_owned(),
            local_date: "2026-06-14".to_owned(),
            start_minute: Some(13 * 60),
            end_minute: Some(14 * 60 + 30),
            text: "Power outage and an afternoon nap".to_owned(),
            context_tags: vec![ContextTag::Other],
        }
    }

    #[test]
    fn bootstraps_an_encrypted_database_and_migrations() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let provider = MemoryKeyProvider::default();

        let store = EncryptedStore::bootstrap(&path, &provider).unwrap();

        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(database_file_looks_encrypted(&path).unwrap());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn persists_batches_and_duplicate_state_across_restarts() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let provider = MemoryKeyProvider::default();
        let source = Uuid::now_v7();
        let batch_id = Uuid::now_v7();

        {
            let mut store = EncryptedStore::bootstrap(&path, &provider).unwrap();
            assert_eq!(
                store.ingest(&batch(source, batch_id, 1)).unwrap(),
                IngestDisposition::Stored
            );
            assert_eq!(store.aggregate_batch_count().unwrap(), 1);
            assert_eq!(store.metric_count().unwrap(), 2);
        }

        let mut reopened = EncryptedStore::bootstrap(&path, &provider).unwrap();
        assert_eq!(
            reopened.ingest(&batch(source, batch_id, 1)).unwrap(),
            IngestDisposition::Duplicate
        );
        assert_eq!(reopened.aggregate_batch_count().unwrap(), 1);
    }

    #[test]
    fn persists_check_ins_and_duplicate_state_across_restarts() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let provider = MemoryKeyProvider::default();
        let check_in_id = Uuid::now_v7();

        {
            let mut store = EncryptedStore::bootstrap(&path, &provider).unwrap();
            assert_eq!(
                store.submit_check_in(&check_in(check_in_id)).unwrap(),
                IngestDisposition::Stored
            );
            assert_eq!(store.check_in_count().unwrap(), 1);
            assert_eq!(store.check_in_context_tag_count().unwrap(), 2);
        }

        let mut reopened = EncryptedStore::bootstrap(&path, &provider).unwrap();
        assert_eq!(
            reopened.submit_check_in(&check_in(check_in_id)).unwrap(),
            IngestDisposition::Duplicate
        );
        assert_eq!(reopened.check_in_count().unwrap(), 1);
        assert_eq!(reopened.check_in_context_tag_count().unwrap(), 2);
    }

    #[test]
    fn saves_updates_lists_and_deletes_annotations() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let provider = MemoryKeyProvider::default();
        let annotation_id = Uuid::now_v7();

        {
            let mut store = EncryptedStore::bootstrap(&path, &provider).unwrap();
            store.save_annotation(&annotation(annotation_id)).unwrap();
            assert_eq!(store.annotation_count().unwrap(), 1);
            assert_eq!(store.annotation_context_tag_count().unwrap(), 1);
        }

        let mut reopened = EncryptedStore::bootstrap(&path, &provider).unwrap();
        let mut updated = annotation(annotation_id);
        updated.text = "Power outage, then a restorative nap".to_owned();
        updated.start_minute = None;
        updated.end_minute = None;
        updated.context_tags = vec![ContextTag::Other, ContextTag::UnusualGoodEvent];
        reopened.save_annotation(&updated).unwrap();

        assert_eq!(reopened.annotations().unwrap(), vec![updated]);
        assert_eq!(reopened.annotation_count().unwrap(), 1);
        assert_eq!(reopened.annotation_context_tag_count().unwrap(), 2);
        assert!(reopened.delete_annotation(annotation_id).unwrap());
        assert!(!reopened.delete_annotation(annotation_id).unwrap());
        assert!(reopened.annotations().unwrap().is_empty());
    }

    #[test]
    fn reports_latest_received_timestamp_by_source_without_content() {
        let temp = TempDir::new().unwrap();
        let provider = MemoryKeyProvider::default();
        let mut store = EncryptedStore::bootstrap(database_path(&temp), &provider).unwrap();
        let source = Uuid::now_v7();
        let browser_received = Utc.with_ymd_and_hms(2026, 6, 14, 12, 20, 0).unwrap();
        let os_received = Utc.with_ymd_and_hms(2026, 6, 14, 12, 21, 0).unwrap();
        let check_in_received = Utc.with_ymd_and_hms(2026, 6, 14, 12, 22, 0).unwrap();

        store
            .ingest_at(&batch(source, Uuid::now_v7(), 1), browser_received)
            .unwrap();
        let mut os_batch = batch(source, Uuid::now_v7(), 2);
        os_batch.metrics = vec![Metric {
            signal: SignalId::OsActiveSeconds,
            value: 300.0,
        }];
        store.ingest_at(&os_batch, os_received).unwrap();
        store
            .submit_check_in_at(&check_in(Uuid::now_v7()), check_in_received)
            .unwrap();

        assert_eq!(
            store.source_activity_timestamps().unwrap(),
            SourceActivityTimestamps {
                browser: Some(browser_received),
                os: Some(os_received),
                check_in: Some(check_in_received),
            }
        );
    }

    #[test]
    fn collection_settings_default_to_disabled_and_persist_transitions() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let provider = MemoryKeyProvider::default();
        let enabled_at = Utc.with_ymd_and_hms(2026, 6, 14, 11, 55, 0).unwrap();

        {
            let mut store = EncryptedStore::bootstrap(&path, &provider).unwrap();
            let initial = store.collection_settings(enabled_at).unwrap();
            assert_eq!(initial.len(), SignalId::ALL.len());
            assert!(initial.iter().all(|setting| !setting.enabled));
            assert!(initial.iter().all(|setting| setting.changed_at.is_none()));

            store
                .set_signal_collection(SignalId::BrowserOpenTabCountMean, true, enabled_at)
                .unwrap();
        }

        let reopened = EncryptedStore::bootstrap(&path, &provider).unwrap();
        let settings = reopened
            .collection_settings(enabled_at + chrono::Duration::minutes(1))
            .unwrap();
        let setting = settings
            .iter()
            .find(|setting| setting.signal == SignalId::BrowserOpenTabCountMean)
            .unwrap();
        assert!(setting.enabled);
        assert_eq!(setting.changed_at, Some(enabled_at));
    }

    #[test]
    fn signal_must_be_enabled_for_the_entire_observation_period() {
        let temp = TempDir::new().unwrap();
        let provider = MemoryKeyProvider::default();
        let mut store = EncryptedStore::bootstrap(database_path(&temp), &provider).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 6, 14, 12, 0, 0).unwrap();
        let end = start + chrono::Duration::minutes(15);
        let signal = SignalId::BrowserTabSwitchCount;

        assert!(!store.signal_enabled_for_period(signal, start, end).unwrap());

        store
            .set_signal_collection(signal, true, start - chrono::Duration::minutes(1))
            .unwrap();
        assert!(store.signal_enabled_for_period(signal, start, end).unwrap());

        store
            .set_signal_collection(signal, false, start + chrono::Duration::minutes(5))
            .unwrap();
        store
            .set_signal_collection(signal, true, start + chrono::Duration::minutes(10))
            .unwrap();
        assert!(!store.signal_enabled_for_period(signal, start, end).unwrap());
        assert!(
            store
                .signal_enabled_for_period(signal, end, end + chrono::Duration::minutes(15))
                .unwrap()
        );
    }

    #[test]
    fn projects_daily_browser_features_in_recorded_time_zone() {
        let temp = TempDir::new().unwrap();
        let provider = MemoryKeyProvider::default();
        let source = Uuid::now_v7();
        let mut store = EncryptedStore::bootstrap(database_path(&temp), &provider).unwrap();

        let mut first = batch_starting_at(
            source,
            Uuid::now_v7(),
            1,
            Utc.with_ymd_and_hms(2026, 6, 14, 2, 30, 0).unwrap(),
        );
        first.metrics = vec![
            Metric {
                signal: SignalId::BrowserOpenTabCountMean,
                value: 10.0,
            },
            Metric {
                signal: SignalId::BrowserOpenTabCountMax,
                value: 12.0,
            },
            Metric {
                signal: SignalId::BrowserTabSwitchCount,
                value: 3.0,
            },
            Metric {
                signal: SignalId::BrowserRetainedAcrossDayCount,
                value: 7.0,
            },
            Metric {
                signal: SignalId::BrowserActiveSeconds,
                value: 300.0,
            },
        ];

        let mut second = batch_starting_at(
            source,
            Uuid::now_v7(),
            2,
            Utc.with_ymd_and_hms(2026, 6, 14, 2, 45, 0).unwrap(),
        );
        second.metrics = vec![
            Metric {
                signal: SignalId::BrowserOpenTabCountMean,
                value: 14.0,
            },
            Metric {
                signal: SignalId::BrowserOpenTabCountMax,
                value: 20.0,
            },
            Metric {
                signal: SignalId::BrowserTabSwitchCount,
                value: 5.0,
            },
            Metric {
                signal: SignalId::BrowserRetainedAcrossDayCount,
                value: 2.0,
            },
            Metric {
                signal: SignalId::BrowserIdleSeconds,
                value: 120.0,
            },
        ];

        store.ingest(&first).unwrap();
        store.ingest(&second).unwrap();

        let daily = store.daily_browser_features().unwrap();
        assert_eq!(
            daily,
            vec![DailyBrowserFeatures {
                local_date: "2026-06-13".to_owned(),
                open_tab_count_mean: Some(12.0),
                open_tab_count_max: Some(20.0),
                tab_switch_count: Some(8.0),
                retained_across_day_count: Some(9.0),
                continuous_scrolling_seconds: None,
                active_seconds: Some(300.0),
                idle_seconds: Some(120.0),
                aggregate_bucket_count: 2,
            }]
        );
    }

    #[test]
    fn projects_daily_os_features_in_recorded_time_zone() {
        let temp = TempDir::new().unwrap();
        let provider = MemoryKeyProvider::default();
        let source = Uuid::now_v7();
        let mut store = EncryptedStore::bootstrap(database_path(&temp), &provider).unwrap();

        let mut first = batch_starting_at(
            source,
            Uuid::now_v7(),
            1,
            Utc.with_ymd_and_hms(2026, 6, 14, 2, 30, 0).unwrap(),
        );
        first.metrics = vec![
            Metric {
                signal: SignalId::OsActiveSeconds,
                value: 600.0,
            },
            Metric {
                signal: SignalId::OsIdleSeconds,
                value: 120.0,
            },
            Metric {
                signal: SignalId::OsLockCount,
                value: 1.0,
            },
        ];

        let mut second = batch_starting_at(
            source,
            Uuid::now_v7(),
            2,
            Utc.with_ymd_and_hms(2026, 6, 14, 2, 45, 0).unwrap(),
        );
        second.metrics = vec![
            Metric {
                signal: SignalId::OsActiveSeconds,
                value: 300.0,
            },
            Metric {
                signal: SignalId::OsUnlockCount,
                value: 1.0,
            },
            Metric {
                signal: SignalId::OsSuspendCount,
                value: 1.0,
            },
            Metric {
                signal: SignalId::OsResumeCount,
                value: 1.0,
            },
        ];

        store.ingest(&first).unwrap();
        store.ingest(&second).unwrap();

        let daily = store.daily_os_features().unwrap();
        assert_eq!(
            daily,
            vec![DailyOsFeatures {
                local_date: "2026-06-13".to_owned(),
                active_seconds: Some(900.0),
                idle_seconds: Some(120.0),
                lock_count: Some(1.0),
                unlock_count: Some(1.0),
                suspend_count: Some(1.0),
                resume_count: Some(1.0),
                aggregate_bucket_count: 2,
            }]
        );
    }

    #[test]
    fn projects_daily_check_in_features() {
        let temp = TempDir::new().unwrap();
        let provider = MemoryKeyProvider::default();
        let mut store = EncryptedStore::bootstrap(database_path(&temp), &provider).unwrap();

        let mut first = check_in(Uuid::now_v7());
        first.sleep_minutes = Some(420);
        first.energy = Some(4);
        let mut second = check_in(Uuid::now_v7());
        second.sleep_minutes = Some(360);
        second.energy = Some(6);

        store.submit_check_in(&first).unwrap();
        store.submit_check_in(&second).unwrap();

        let daily = store.daily_check_in_features().unwrap();
        assert_eq!(
            daily,
            vec![DailyCheckInFeatures {
                local_date: "2026-06-14".to_owned(),
                sleep_minutes: Some(390.0),
                mood: Some(5.0),
                energy: Some(5.0),
                irritability: Some(2.0),
                concentration: Some(4.0),
                impulsivity: Some(3.0),
                check_in_count: 2,
                context_tags: vec![ContextTag::Deadline, ContextTag::NewsCycle],
            }]
        );
    }

    #[test]
    fn rejects_stale_sequences_transactionally() {
        let temp = TempDir::new().unwrap();
        let provider = MemoryKeyProvider::default();
        let mut store = EncryptedStore::bootstrap(database_path(&temp), &provider).unwrap();
        let source = Uuid::now_v7();

        store.ingest(&batch(source, Uuid::now_v7(), 2)).unwrap();
        assert!(matches!(
            store.ingest(&batch(source, Uuid::now_v7(), 1)),
            Err(StorageError::SequenceConflict)
        ));
        assert_eq!(store.aggregate_batch_count().unwrap(), 1);
    }

    #[test]
    fn refuses_an_existing_database_when_the_key_is_missing() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let provider = MemoryKeyProvider::default();
        drop(EncryptedStore::bootstrap(&path, &provider).unwrap());
        provider.delete().unwrap();

        assert!(matches!(
            EncryptedStore::bootstrap(&path, &provider),
            Err(StorageError::MissingDatabaseKey)
        ));
    }

    #[test]
    fn refuses_a_wrong_database_key() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let correct_key = DatabaseKey::from_bytes([7; DATABASE_KEY_BYTES]);
        drop(EncryptedStore::open(&path, &correct_key).unwrap());

        let wrong_key = DatabaseKey::from_bytes([8; DATABASE_KEY_BYTES]);
        assert!(matches!(
            EncryptedStore::open(&path, &wrong_key),
            Err(StorageError::InvalidDatabaseKey(_) | StorageError::Database(_))
        ));
    }

    #[test]
    fn destroy_local_profile_removes_database_sidecars_and_key() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let provider = MemoryKeyProvider::default();

        {
            let mut store = EncryptedStore::bootstrap(&path, &provider).unwrap();
            store
                .set_signal_collection(
                    SignalId::BrowserTabSwitchCount,
                    true,
                    Utc.with_ymd_and_hms(2026, 6, 14, 11, 55, 0).unwrap(),
                )
                .unwrap();
            store
                .ingest(&batch(Uuid::now_v7(), Uuid::now_v7(), 1))
                .unwrap();
        }
        let wal_path = path.with_file_name("mindcanary.db-wal");
        let shm_path = path.with_file_name("mindcanary.db-shm");
        let journal_path = path.with_file_name("mindcanary.db-journal");
        std::fs::write(&wal_path, b"wal").unwrap();
        std::fs::write(&shm_path, b"shm").unwrap();
        std::fs::write(&journal_path, b"journal").unwrap();
        assert!(provider.load().unwrap().is_some());

        let report = destroy_local_profile(&path, &provider).unwrap();

        assert_eq!(report.database_path, path);
        assert!(report.removed_files.contains(&path));
        assert!(report.removed_files.contains(&wal_path));
        assert!(report.removed_files.contains(&shm_path));
        assert!(report.removed_files.contains(&journal_path));
        for removed_path in [&path, &wal_path, &shm_path, &journal_path] {
            assert!(
                !removed_path.exists(),
                "{} should be removed",
                removed_path.display()
            );
        }
        assert!(provider.load().unwrap().is_none());
    }

    #[test]
    fn destroy_local_profile_is_idempotent_for_missing_files_and_key() {
        let temp = TempDir::new().unwrap();
        let path = database_path(&temp);
        let provider = MemoryKeyProvider::default();

        let report = destroy_local_profile(&path, &provider).unwrap();

        assert_eq!(report.database_path, path);
        assert!(report.removed_files.is_empty());
        assert!(provider.load().unwrap().is_none());
    }

    #[test]
    fn clear_all_removes_canonical_aggregate_records() {
        let temp = TempDir::new().unwrap();
        let provider = MemoryKeyProvider::default();
        let mut store = EncryptedStore::bootstrap(database_path(&temp), &provider).unwrap();
        store
            .ingest(&batch(Uuid::now_v7(), Uuid::now_v7(), 1))
            .unwrap();
        store.submit_check_in(&check_in(Uuid::now_v7())).unwrap();

        store.clear_all().unwrap();

        assert_eq!(store.aggregate_batch_count().unwrap(), 0);
        assert_eq!(store.metric_count().unwrap(), 0);
        assert_eq!(store.check_in_count().unwrap(), 0);
        assert_eq!(store.check_in_context_tag_count().unwrap(), 0);
    }

    #[test]
    fn deletes_only_one_signal_and_preserves_replay_protection() {
        let temp = TempDir::new().unwrap();
        let provider = MemoryKeyProvider::default();
        let mut store = EncryptedStore::bootstrap(database_path(&temp), &provider).unwrap();
        let source = Uuid::now_v7();
        let first = batch(source, Uuid::now_v7(), 1);
        let mut second = batch(source, Uuid::now_v7(), 2);
        second.metrics = vec![Metric {
            signal: SignalId::BrowserTabSwitchCount,
            value: 9.0,
        }];
        store.ingest(&first).unwrap();
        store.ingest(&second).unwrap();
        store.submit_check_in(&check_in(Uuid::now_v7())).unwrap();
        store
            .set_signal_collection(
                SignalId::BrowserTabSwitchCount,
                true,
                Utc.with_ymd_and_hms(2026, 6, 14, 11, 0, 0).unwrap(),
            )
            .unwrap();

        assert_eq!(
            store
                .signal_record_summary(SignalId::BrowserTabSwitchCount)
                .unwrap(),
            SignalRecordSummary {
                metric_record_count: 2,
                affected_batch_count: 2,
            }
        );
        let deleted = store
            .delete_signal_records(SignalId::BrowserTabSwitchCount)
            .unwrap();

        assert_eq!(deleted.metric_record_count, 2);
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
        assert!(
            store
                .collection_settings(Utc.with_ymd_and_hms(2026, 6, 14, 12, 0, 0).unwrap())
                .unwrap()
                .iter()
                .any(|setting| {
                    setting.signal == SignalId::BrowserTabSwitchCount && setting.enabled
                })
        );
        assert!(matches!(
            store.ingest(&second),
            Err(StorageError::SequenceConflict)
        ));
    }

    #[test]
    fn encrypted_backup_verifies_and_restores_complete_local_records() {
        let temp = TempDir::new().unwrap();
        let source_provider = MemoryKeyProvider::default();
        let mut source =
            EncryptedStore::bootstrap(temp.path().join("source.db"), &source_provider).unwrap();
        source
            .ingest(&batch(Uuid::now_v7(), Uuid::now_v7(), 1))
            .unwrap();
        source.submit_check_in(&check_in(Uuid::now_v7())).unwrap();
        source.save_annotation(&annotation(Uuid::now_v7())).unwrap();
        source
            .set_signal_collection(
                SignalId::BrowserTabSwitchCount,
                true,
                Utc.with_ymd_and_hms(2026, 6, 14, 11, 0, 0).unwrap(),
            )
            .unwrap();

        let backup_path = temp.path().join("mindcanary-2026-06-19.mcbak");
        let created_at = Utc.with_ymd_and_hms(2026, 6, 19, 6, 0, 0).unwrap();
        let backup = source
            .create_encrypted_backup(&backup_path, created_at)
            .unwrap();

        assert_eq!(backup.created_at, created_at);
        assert_eq!(backup.format_version, BACKUP_FORMAT_VERSION);
        assert_eq!(backup.recovery_secret.replace('-', "").len(), 64);
        assert!(database_file_looks_encrypted(&backup_path).unwrap());
        assert_eq!(
            EncryptedStore::verify_encrypted_backup(&backup_path, &backup.recovery_secret).unwrap(),
            EncryptedBackupMetadata {
                created_at,
                format_version: BACKUP_FORMAT_VERSION,
                schema_version: CURRENT_SCHEMA_VERSION,
            }
        );
        assert!(matches!(
            EncryptedStore::verify_encrypted_backup(
                &backup_path,
                "0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000"
            ),
            Err(StorageError::InvalidBackupKey(_))
        ));

        let destination_provider = MemoryKeyProvider::default();
        let mut destination =
            EncryptedStore::bootstrap(temp.path().join("destination.db"), &destination_provider)
                .unwrap();
        let restored = destination
            .restore_encrypted_backup(&backup_path, &backup.recovery_secret)
            .unwrap();

        assert_eq!(restored.aggregate_batch_count, 1);
        assert_eq!(restored.aggregate_metric_count, 2);
        assert_eq!(restored.check_in_count, 1);
        assert_eq!(restored.annotation_count, 1);
        assert_eq!(destination.annotations().unwrap().len(), 1);
        assert!(
            destination
                .collection_settings(created_at)
                .unwrap()
                .iter()
                .any(|setting| setting.signal == SignalId::BrowserTabSwitchCount && setting.enabled)
        );
    }

    #[test]
    fn encrypted_backup_restore_refuses_to_replace_existing_records() {
        let temp = TempDir::new().unwrap();
        let source_provider = MemoryKeyProvider::default();
        let mut source =
            EncryptedStore::bootstrap(temp.path().join("source.db"), &source_provider).unwrap();
        source.submit_check_in(&check_in(Uuid::now_v7())).unwrap();
        let backup = source
            .create_encrypted_backup(temp.path().join("backup.mcbak"), Utc::now())
            .unwrap();

        let destination_provider = MemoryKeyProvider::default();
        let mut destination =
            EncryptedStore::bootstrap(temp.path().join("destination.db"), &destination_provider)
                .unwrap();
        destination
            .save_annotation(&annotation(Uuid::now_v7()))
            .unwrap();

        assert!(matches!(
            destination.restore_encrypted_backup(&backup.path, &backup.recovery_secret),
            Err(StorageError::RestoreRequiresEmptyRecords)
        ));
        assert_eq!(destination.annotation_count().unwrap(), 1);
    }

    #[test]
    fn maximum_cross_platform_sequence_fits_storage() {
        assert!(i64::try_from(MAX_SAFE_SEQUENCE).is_ok());
    }
}
