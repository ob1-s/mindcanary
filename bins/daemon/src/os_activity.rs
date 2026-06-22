use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration as StdDuration,
};

use chrono::{DateTime, Duration, TimeZone as _, Utc};
use futures_util::StreamExt as _;
use mindcanary_protocol::{
    AggregateBatch, Metric, ObservationPeriod, PROTOCOL_VERSION, ProtocolRequest, ProtocolResponse,
    SignalId,
};
use uuid::Uuid;

use super::{DaemonState, OsAdapterRuntimeStatus};

const BUCKET_MINUTES: i64 = 15;
const SAMPLE_SECONDS: u64 = 30;
const MAX_ATTRIBUTABLE_GAP_SECONDS: i64 = 90;
const IDLE_THRESHOLD_MILLIS: u64 = 5 * 60 * 1_000;
const DBUS_TIMEOUT_SECONDS: u64 = 5;

const MUTTER_IDLE_SERVICE: &str = "org.gnome.Mutter.IdleMonitor";
const MUTTER_IDLE_PATH: &str = "/org/gnome/Mutter/IdleMonitor/Core";
const MUTTER_IDLE_INTERFACE: &str = "org.gnome.Mutter.IdleMonitor";
const LIFECYCLE_RECONNECT_SECONDS: u64 = 5;

#[zbus::proxy(
    default_service = "org.gnome.ScreenSaver",
    default_path = "/org/gnome/ScreenSaver",
    interface = "org.gnome.ScreenSaver"
)]
trait GnomeScreenSaver {
    #[zbus(signal)]
    fn active_changed(&self, active: bool) -> zbus::Result<()>;
}

#[zbus::proxy(
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1",
    interface = "org.freedesktop.login1.Manager"
)]
trait Login1Manager {
    #[zbus(signal)]
    fn prepare_for_sleep(&self, preparing: bool) -> zbus::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivityState {
    Active,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CompletedBucket {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    active_seconds: f64,
    idle_seconds: f64,
}

#[derive(Debug)]
struct ActivityReducer {
    bucket_start: DateTime<Utc>,
    cursor: DateTime<Utc>,
    state: ActivityState,
    active_seconds: f64,
    idle_seconds: f64,
    eligible: bool,
}

impl ActivityReducer {
    fn new(observed_at: DateTime<Utc>, state: ActivityState) -> Self {
        let bucket_start = aligned_bucket_start(observed_at);
        Self {
            bucket_start,
            cursor: observed_at,
            state,
            active_seconds: 0.0,
            idle_seconds: 0.0,
            eligible: observed_at == bucket_start,
        }
    }

    fn advance(
        &mut self,
        observed_at: DateTime<Utc>,
        state: ActivityState,
    ) -> Vec<CompletedBucket> {
        if observed_at <= self.cursor {
            self.state = state;
            return Vec::new();
        }

        if observed_at - self.cursor > Duration::seconds(MAX_ATTRIBUTABLE_GAP_SECONDS) {
            *self = Self::new(observed_at, state);
            return Vec::new();
        }

        let mut completed = Vec::new();
        while self.cursor < observed_at {
            let bucket_end = self.bucket_start + Duration::minutes(BUCKET_MINUTES);
            let segment_end = observed_at.min(bucket_end);
            let seconds = (segment_end - self.cursor)
                .to_std()
                .map_or(0.0, |duration| duration.as_secs_f64());
            match self.state {
                ActivityState::Active => self.active_seconds += seconds,
                ActivityState::Idle => self.idle_seconds += seconds,
            }
            self.cursor = segment_end;

            if self.cursor == bucket_end {
                if self.eligible {
                    completed.push(CompletedBucket {
                        start: self.bucket_start,
                        end: bucket_end,
                        active_seconds: self.active_seconds,
                        idle_seconds: self.idle_seconds,
                    });
                }
                self.bucket_start = bucket_end;
                self.active_seconds = 0.0;
                self.idle_seconds = 0.0;
                self.eligible = true;
            }
        }
        self.state = state;
        completed
    }
}

#[derive(Debug, Clone, Copy)]
struct EnabledSignals {
    active: bool,
    idle: bool,
}

impl EnabledSignals {
    const fn any(self) -> bool {
        self.active || self.idle
    }
}

struct GnomeIdleMonitor {
    connection: zbus::Connection,
}

impl GnomeIdleMonitor {
    async fn connect() -> zbus::Result<Self> {
        Ok(Self {
            connection: zbus::Connection::session().await?,
        })
    }

    async fn idle_millis(&self) -> zbus::Result<u64> {
        let proxy = zbus::Proxy::new(
            &self.connection,
            MUTTER_IDLE_SERVICE,
            MUTTER_IDLE_PATH,
            MUTTER_IDLE_INTERFACE,
        )
        .await?;
        proxy.call("GetIdletime", &()).await
    }
}

pub(super) async fn run(state: Arc<DaemonState>) {
    tokio::spawn(run_lock_events(state.clone()));
    tokio::spawn(run_sleep_events(state.clone()));
    let source_instance_id = Uuid::now_v7();
    let time_zone = local_time_zone();
    let mut sequence = 0_u64;
    let mut monitor = None;
    let mut reducer = None;
    let mut ticker = tokio::time::interval(StdDuration::from_secs(SAMPLE_SECONDS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        let now = Utc::now();
        let enabled = enabled_signals(&state, now);

        if monitor.is_none() {
            match GnomeIdleMonitor::connect().await {
                Ok(candidate) => match read_idle_millis(&candidate).await {
                    Ok(idle_millis) => {
                        set_runtime_status(&state, OsAdapterRuntimeStatus::Available);
                        monitor = Some(candidate);
                        if enabled.any() {
                            reducer = Some(ActivityReducer::new(now, activity_state(idle_millis)));
                        }
                    }
                    Err(()) => {
                        set_runtime_status(&state, OsAdapterRuntimeStatus::Unavailable);
                    }
                },
                Err(_) => set_runtime_status(&state, OsAdapterRuntimeStatus::Unavailable),
            }
        } else if !enabled.any() {
            reducer = None;
        } else {
            let Ok(idle_millis) = read_idle_millis(monitor.as_ref().expect("monitor exists")).await
            else {
                set_runtime_status(&state, OsAdapterRuntimeStatus::Unavailable);
                monitor = None;
                reducer = None;
                continue;
            };
            set_runtime_status(&state, OsAdapterRuntimeStatus::Available);
            let activity = activity_state(idle_millis);
            let current = reducer.get_or_insert_with(|| ActivityReducer::new(now, activity));
            for bucket in current.advance(now, activity) {
                let Some(next_sequence) = sequence.checked_add(1) else {
                    reducer = None;
                    continue;
                };
                sequence = next_sequence;
                ingest_bucket(
                    &state,
                    bucket,
                    enabled,
                    source_instance_id,
                    sequence,
                    &time_zone,
                    now,
                );
            }
        }
    }
}

async fn run_lock_events(state: Arc<DaemonState>) {
    loop {
        let connected = async {
            let connection = zbus::Connection::session().await?;
            let proxy = GnomeScreenSaverProxy::new(&connection).await?;
            let mut events = proxy.receive_active_changed().await?;
            set_lifecycle_status(&state, Some(true), None);
            let source_instance_id = Uuid::now_v7();
            let mut sequence = 0_u64;
            while let Some(event) = events.next().await {
                let Ok(arguments) = event.args() else {
                    continue;
                };
                let Some(next_sequence) = sequence.checked_add(1) else {
                    break;
                };
                sequence = next_sequence;
                ingest_lifecycle_event(
                    &state,
                    lock_signal(arguments.active),
                    source_instance_id,
                    sequence,
                    &local_time_zone(),
                    Utc::now(),
                );
            }
            Ok::<(), zbus::Error>(())
        }
        .await;
        let _ = connected;
        set_lifecycle_status(&state, Some(false), None);
        tokio::time::sleep(StdDuration::from_secs(LIFECYCLE_RECONNECT_SECONDS)).await;
    }
}

async fn run_sleep_events(state: Arc<DaemonState>) {
    loop {
        let connected = async {
            let connection = zbus::Connection::system().await?;
            let proxy = Login1ManagerProxy::new(&connection).await?;
            let mut events = proxy.receive_prepare_for_sleep().await?;
            set_lifecycle_status(&state, None, Some(true));
            let source_instance_id = Uuid::now_v7();
            let mut sequence = 0_u64;
            while let Some(event) = events.next().await {
                let Ok(arguments) = event.args() else {
                    continue;
                };
                let Some(next_sequence) = sequence.checked_add(1) else {
                    break;
                };
                sequence = next_sequence;
                ingest_lifecycle_event(
                    &state,
                    sleep_signal(arguments.preparing),
                    source_instance_id,
                    sequence,
                    &local_time_zone(),
                    Utc::now(),
                );
            }
            Ok::<(), zbus::Error>(())
        }
        .await;
        let _ = connected;
        set_lifecycle_status(&state, None, Some(false));
        tokio::time::sleep(StdDuration::from_secs(LIFECYCLE_RECONNECT_SECONDS)).await;
    }
}

const fn lock_signal(active: bool) -> SignalId {
    if active {
        SignalId::OsLockCount
    } else {
        SignalId::OsUnlockCount
    }
}

const fn sleep_signal(preparing: bool) -> SignalId {
    if preparing {
        SignalId::OsSuspendCount
    } else {
        SignalId::OsResumeCount
    }
}

fn ingest_lifecycle_event(
    state: &DaemonState,
    signal: SignalId,
    source_instance_id: Uuid,
    sequence: u64,
    time_zone: &str,
    observed_at: DateTime<Utc>,
) {
    let response = state.handle_request(
        ProtocolRequest::IngestAggregate {
            protocol_version: PROTOCOL_VERSION,
            batch: AggregateBatch {
                batch_id: Uuid::now_v7(),
                source_instance_id,
                sequence,
                period: ObservationPeriod {
                    start: observed_at,
                    end: observed_at + Duration::milliseconds(1),
                    time_zone: time_zone.to_owned(),
                },
                metrics: vec![Metric { signal, value: 1.0 }],
            },
        },
        observed_at,
    );
    if matches!(response, ProtocolResponse::Error { .. }) {
        eprintln!("os_lifecycle_event_rejected");
    }
}

fn set_lifecycle_status(
    state: &DaemonState,
    lock_events: Option<bool>,
    sleep_events: Option<bool>,
) {
    if let Ok(mut status) = state.os_lifecycle_status.lock() {
        if let Some(available) = lock_events {
            status.lock_events = available;
        }
        if let Some(available) = sleep_events {
            status.sleep_events = available;
        }
    }
}

async fn read_idle_millis(monitor: &GnomeIdleMonitor) -> Result<u64, ()> {
    tokio::time::timeout(
        StdDuration::from_secs(DBUS_TIMEOUT_SECONDS),
        monitor.idle_millis(),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}

fn enabled_signals(state: &DaemonState, now: DateTime<Utc>) -> EnabledSignals {
    let Ok(store) = state.store.lock() else {
        return EnabledSignals {
            active: false,
            idle: false,
        };
    };
    let Ok(settings) = store.collection_settings(now) else {
        return EnabledSignals {
            active: false,
            idle: false,
        };
    };
    EnabledSignals {
        active: settings
            .iter()
            .any(|setting| setting.signal == SignalId::OsActiveSeconds && setting.enabled),
        idle: settings
            .iter()
            .any(|setting| setting.signal == SignalId::OsIdleSeconds && setting.enabled),
    }
}

fn ingest_bucket(
    state: &DaemonState,
    bucket: CompletedBucket,
    enabled: EnabledSignals,
    source_instance_id: Uuid,
    sequence: u64,
    time_zone: &str,
    now: DateTime<Utc>,
) {
    let mut metrics = Vec::with_capacity(2);
    if enabled.active {
        metrics.push(Metric {
            signal: SignalId::OsActiveSeconds,
            value: bucket.active_seconds,
        });
    }
    if enabled.idle {
        metrics.push(Metric {
            signal: SignalId::OsIdleSeconds,
            value: bucket.idle_seconds,
        });
    }
    if metrics.is_empty() {
        return;
    }

    let response = state.handle_request(
        ProtocolRequest::IngestAggregate {
            protocol_version: PROTOCOL_VERSION,
            batch: AggregateBatch {
                batch_id: Uuid::now_v7(),
                source_instance_id,
                sequence,
                period: ObservationPeriod {
                    start: bucket.start,
                    end: bucket.end,
                    time_zone: time_zone.to_owned(),
                },
                metrics,
            },
        },
        now,
    );
    if matches!(response, ProtocolResponse::Error { .. }) {
        eprintln!("os_activity_batch_rejected");
    }
}

fn set_runtime_status(state: &DaemonState, status: OsAdapterRuntimeStatus) {
    if let Ok(mut current) = state.os_adapter_status.lock() {
        *current = status;
    }
}

const fn activity_state(idle_millis: u64) -> ActivityState {
    if idle_millis >= IDLE_THRESHOLD_MILLIS {
        ActivityState::Idle
    } else {
        ActivityState::Active
    }
}

fn aligned_bucket_start(observed_at: DateTime<Utc>) -> DateTime<Utc> {
    let bucket_seconds = BUCKET_MINUTES * 60;
    let timestamp = observed_at.timestamp();
    let aligned = timestamp - timestamp.rem_euclid(bucket_seconds);
    Utc.timestamp_opt(aligned, 0)
        .single()
        .expect("aligned UTC timestamp must be representable")
}

fn local_time_zone() -> String {
    time_zone_from(
        std::env::var("TZ").ok().as_deref(),
        Path::new("/etc/localtime"),
    )
}

fn time_zone_from(tz: Option<&str>, localtime_path: &Path) -> String {
    if let Some(value) = tz.filter(|value| is_time_zone_name(value)) {
        return value.to_owned();
    }

    std::fs::canonicalize(localtime_path)
        .ok()
        .and_then(|path| time_zone_from_zoneinfo_path(&path))
        .unwrap_or_else(|| "UTC".to_owned())
}

fn time_zone_from_zoneinfo_path(path: &Path) -> Option<String> {
    let marker = Path::new("zoneinfo");
    let components = path.components().collect::<Vec<_>>();
    let marker_index = components
        .iter()
        .position(|component| component.as_os_str() == marker.as_os_str())?;
    let value = components
        .iter()
        .skip(marker_index + 1)
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned();
    is_time_zone_name(&value).then_some(value)
}

fn is_time_zone_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('/')
        && !value.contains("..")
        && value.parse::<chrono_tz::Tz>().is_ok()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use mindcanary_protocol::{LocalDataSummary, PROTOCOL_VERSION, ProtocolRequest};
    use mindcanary_storage::{DatabaseKey, EncryptedStore};
    use tempfile::TempDir;

    use super::*;

    fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 15, hour, minute, second)
            .unwrap()
    }

    #[test]
    fn drops_the_partial_start_bucket_then_emits_aligned_totals() {
        let mut reducer = ActivityReducer::new(at(10, 7, 0), ActivityState::Active);
        let completed = drive_interval(
            &mut reducer,
            at(10, 7, 0),
            at(10, 30, 0),
            ActivityState::Active,
        );
        assert_eq!(
            completed,
            vec![CompletedBucket {
                start: at(10, 15, 0),
                end: at(10, 30, 0),
                active_seconds: 900.0,
                idle_seconds: 0.0,
            }]
        );
    }

    #[test]
    fn splits_activity_at_state_changes_and_bucket_boundaries() {
        let mut reducer = ActivityReducer::new(at(10, 0, 0), ActivityState::Active);
        let mut completed = drive_interval(
            &mut reducer,
            at(10, 0, 0),
            at(10, 5, 0),
            ActivityState::Active,
        );
        assert!(
            reducer
                .advance(at(10, 5, 0), ActivityState::Idle)
                .is_empty()
        );
        completed.extend(drive_interval(
            &mut reducer,
            at(10, 5, 0),
            at(10, 15, 0),
            ActivityState::Idle,
        ));

        assert!((completed[0].active_seconds - 300.0).abs() < 0.001);
        assert!((completed[0].idle_seconds - 600.0).abs() < 0.001);
    }

    #[test]
    fn discards_interrupted_buckets_instead_of_guessing() {
        let mut reducer = ActivityReducer::new(at(10, 0, 0), ActivityState::Active);
        assert!(
            reducer
                .advance(at(10, 2, 0), ActivityState::Active)
                .is_empty()
        );
        assert!(
            reducer
                .advance(at(10, 10, 0), ActivityState::Idle)
                .is_empty()
        );
        assert!(
            reducer
                .advance(at(10, 15, 0), ActivityState::Idle)
                .is_empty()
        );
    }

    #[test]
    fn resolves_only_valid_iana_time_zone_names() {
        assert_eq!(
            time_zone_from(Some("America/Sao_Paulo"), Path::new("/missing")),
            "America/Sao_Paulo"
        );
        assert_eq!(
            time_zone_from(Some("../../etc/passwd"), Path::new("/missing")),
            "UTC"
        );
    }

    #[test]
    fn uses_a_five_minute_idle_threshold() {
        assert_eq!(
            activity_state(IDLE_THRESHOLD_MILLIS - 1),
            ActivityState::Active
        );
        assert_eq!(activity_state(IDLE_THRESHOLD_MILLIS), ActivityState::Idle);
    }

    #[test]
    fn lifecycle_signals_map_without_interpreting_user_activity() {
        assert_eq!(lock_signal(true), SignalId::OsLockCount);
        assert_eq!(lock_signal(false), SignalId::OsUnlockCount);
        assert_eq!(sleep_signal(true), SignalId::OsSuspendCount);
        assert_eq!(sleep_signal(false), SignalId::OsResumeCount);
    }

    #[test]
    fn lifecycle_event_ingestion_requires_explicit_signal_consent() {
        let temp = TempDir::new().unwrap();
        let key = DatabaseKey::from_bytes([29; 32]);
        let store = EncryptedStore::open(temp.path().join("mindcanary.db"), &key).unwrap();
        let state = DaemonState::new(store);
        let source = Uuid::now_v7();

        ingest_lifecycle_event(
            &state,
            SignalId::OsLockCount,
            source,
            1,
            "UTC",
            at(10, 0, 0),
        );
        assert_eq!(summary(&state).aggregate_metric_count, 0);

        let _ = state.handle_request(
            ProtocolRequest::SetSignalCollection {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::OsLockCount,
                enabled: true,
            },
            at(9, 59, 0),
        );
        ingest_lifecycle_event(
            &state,
            SignalId::OsLockCount,
            source,
            2,
            "UTC",
            at(10, 1, 0),
        );

        assert_eq!(summary(&state).aggregate_metric_count, 1);
        let store = state.store.lock().unwrap();
        assert_eq!(store.daily_os_features().unwrap()[0].lock_count, Some(1.0));
    }

    #[test]
    fn os_bucket_ingestion_still_requires_daemon_signal_consent() {
        let temp = TempDir::new().unwrap();
        let key = DatabaseKey::from_bytes([23; 32]);
        let store = EncryptedStore::open(temp.path().join("mindcanary.db"), &key).unwrap();
        let state = DaemonState::new(store);
        let bucket = CompletedBucket {
            start: at(10, 0, 0),
            end: at(10, 15, 0),
            active_seconds: 600.0,
            idle_seconds: 300.0,
        };

        ingest_bucket(
            &state,
            bucket,
            EnabledSignals {
                active: true,
                idle: true,
            },
            Uuid::now_v7(),
            1,
            "UTC",
            at(10, 16, 0),
        );
        assert_eq!(summary(&state).aggregate_metric_count, 0);

        let response = state.handle_request(
            ProtocolRequest::SetSignalCollection {
                protocol_version: PROTOCOL_VERSION,
                signal: SignalId::OsActiveSeconds,
                enabled: true,
            },
            at(9, 59, 0),
        );
        assert!(matches!(
            response,
            ProtocolResponse::CollectionSettings { .. }
        ));

        ingest_bucket(
            &state,
            bucket,
            EnabledSignals {
                active: true,
                idle: true,
            },
            Uuid::now_v7(),
            1,
            "UTC",
            at(10, 16, 0),
        );
        assert_eq!(summary(&state).aggregate_metric_count, 1);
    }

    fn summary(state: &DaemonState) -> LocalDataSummary {
        let response = state.handle_request(
            ProtocolRequest::GetLocalDataSummary {
                protocol_version: PROTOCOL_VERSION,
            },
            at(10, 20, 0),
        );
        let ProtocolResponse::LocalDataSummary { summary, .. } = response else {
            panic!("expected local data summary");
        };
        summary
    }

    fn drive_interval(
        reducer: &mut ActivityReducer,
        mut cursor: DateTime<Utc>,
        end: DateTime<Utc>,
        state: ActivityState,
    ) -> Vec<CompletedBucket> {
        let mut completed = Vec::new();
        while cursor < end {
            cursor += Duration::seconds(i64::try_from(SAMPLE_SECONDS).unwrap());
            let observed_at = cursor.min(end);
            completed.extend(reducer.advance(observed_at, state));
        }
        completed
    }
}
