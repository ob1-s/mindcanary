use std::path::{Path, PathBuf};

use mindcanary_protocol::{
    AnnotationRecord, CheckInRecord, DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT,
    DEFAULT_DAILY_TIMELINE_LIMIT, MAX_FRAME_BYTES, PROTOCOL_VERSION, ProtocolRequest,
    ProtocolResponse, SignalId,
};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonClient {
    socket_path: PathBuf,
}

impl DaemonClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub fn for_default_socket() -> Self {
        Self::new(default_socket_path())
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn send(&self, request: &ProtocolRequest) -> Result<ProtocolResponse, ClientError> {
        send_request(&self.socket_path, request).await
    }

    pub async fn health(&self) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::Health {
            protocol_version: PROTOCOL_VERSION,
        })
        .await
    }

    pub async fn source_status(&self) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::GetSourceStatus {
            protocol_version: PROTOCOL_VERSION,
        })
        .await
    }

    pub async fn daily_rhythm_insights(
        &self,
        limit: Option<u16>,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::GetDailyRhythmInsights {
            protocol_version: PROTOCOL_VERSION,
            limit: limit.or(Some(DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT)),
        })
        .await
    }

    pub async fn daily_timeline(
        &self,
        limit: Option<u16>,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::GetDailyTimeline {
            protocol_version: PROTOCOL_VERSION,
            limit: limit.or(Some(DEFAULT_DAILY_TIMELINE_LIMIT)),
        })
        .await
    }

    pub async fn local_data_summary(&self) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::GetLocalDataSummary {
            protocol_version: PROTOCOL_VERSION,
        })
        .await
    }

    pub async fn collection_settings(&self) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::GetCollectionSettings {
            protocol_version: PROTOCOL_VERSION,
        })
        .await
    }

    pub async fn platform_capabilities(&self) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::GetPlatformCapabilities {
            protocol_version: PROTOCOL_VERSION,
        })
        .await
    }

    pub async fn submit_check_in(
        &self,
        check_in: CheckInRecord,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::SubmitCheckIn {
            protocol_version: PROTOCOL_VERSION,
            check_in,
        })
        .await
    }

    pub async fn save_annotation(
        &self,
        annotation: AnnotationRecord,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::SaveAnnotation {
            protocol_version: PROTOCOL_VERSION,
            annotation,
        })
        .await
    }

    pub async fn prepare_delete_annotation(
        &self,
        annotation_id: uuid::Uuid,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::PrepareDeleteAnnotation {
            protocol_version: PROTOCOL_VERSION,
            annotation_id,
        })
        .await
    }

    pub async fn delete_annotation(
        &self,
        annotation_id: uuid::Uuid,
        confirmation_token: uuid::Uuid,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::DeleteAnnotation {
            protocol_version: PROTOCOL_VERSION,
            annotation_id,
            confirmation_token,
        })
        .await
    }

    pub async fn set_signal_collection(
        &self,
        signal: SignalId,
        enabled: bool,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::SetSignalCollection {
            protocol_version: PROTOCOL_VERSION,
            signal,
            enabled,
        })
        .await
    }

    pub async fn prepare_delete_signal_records(
        &self,
        signal: SignalId,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::PrepareDeleteSignalRecords {
            protocol_version: PROTOCOL_VERSION,
            signal,
        })
        .await
    }

    pub async fn delete_signal_records(
        &self,
        signal: SignalId,
        confirmation_token: uuid::Uuid,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::DeleteSignalRecords {
            protocol_version: PROTOCOL_VERSION,
            signal,
            confirmation_token,
        })
        .await
    }

    pub async fn prepare_clear_local_records(&self) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::PrepareClearLocalRecords {
            protocol_version: PROTOCOL_VERSION,
        })
        .await
    }

    pub async fn prepare_export_local_records(&self) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::PrepareExportLocalRecords {
            protocol_version: PROTOCOL_VERSION,
        })
        .await
    }

    pub async fn export_local_records(
        &self,
        confirmation_token: uuid::Uuid,
        export_directory: String,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::ExportLocalRecords {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token,
            export_directory,
        })
        .await
    }

    pub async fn prepare_create_local_backup(&self) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::PrepareCreateLocalBackup {
            protocol_version: PROTOCOL_VERSION,
        })
        .await
    }

    pub async fn create_local_backup(
        &self,
        confirmation_token: uuid::Uuid,
        backup_path: String,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::CreateLocalBackup {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token,
            backup_path,
        })
        .await
    }

    pub async fn verify_local_backup(
        &self,
        backup_path: String,
        recovery_secret: String,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::VerifyLocalBackup {
            protocol_version: PROTOCOL_VERSION,
            backup_path,
            recovery_secret,
        })
        .await
    }

    pub async fn restore_local_backup(
        &self,
        backup_path: String,
        recovery_secret: String,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::RestoreLocalBackup {
            protocol_version: PROTOCOL_VERSION,
            backup_path,
            recovery_secret,
        })
        .await
    }

    pub async fn clear_local_records(
        &self,
        confirmation_token: uuid::Uuid,
    ) -> Result<ProtocolResponse, ClientError> {
        self.send(&ProtocolRequest::ClearLocalRecords {
            protocol_version: PROTOCOL_VERSION,
            confirmation_token,
        })
        .await
    }
}

pub async fn send_request(
    socket_path: impl AsRef<Path>,
    request: &ProtocolRequest,
) -> Result<ProtocolResponse, ClientError> {
    let payload = serde_json::to_vec(request).map_err(ClientError::EncodeRequest)?;
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(ClientError::Connect)?;

    write_frame(&mut stream, &payload).await?;
    let response = read_frame(&mut stream).await?;
    serde_json::from_slice::<ProtocolResponse>(&response).map_err(ClientError::DecodeResponse)
}

pub fn default_socket_path() -> PathBuf {
    let runtime_root =
        std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    runtime_root.join("mindcanary").join("mindcanaryd.sock")
}

async fn write_frame(stream: &mut UnixStream, payload: &[u8]) -> Result<(), ClientError> {
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(ClientError::InvalidRequestLength {
            bytes: payload.len(),
        });
    }

    let length = u32::try_from(payload.len()).map_err(|_| ClientError::InvalidRequestLength {
        bytes: payload.len(),
    })?;
    stream
        .write_all(&length.to_be_bytes())
        .await
        .map_err(ClientError::WriteRequest)?;
    stream
        .write_all(payload)
        .await
        .map_err(ClientError::WriteRequest)?;
    stream.flush().await.map_err(ClientError::WriteRequest)
}

async fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>, ClientError> {
    let mut length_bytes = [0_u8; 4];
    stream
        .read_exact(&mut length_bytes)
        .await
        .map_err(ClientError::ReadResponse)?;
    let length = u32::from_be_bytes(length_bytes) as usize;

    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(ClientError::InvalidResponseLength { bytes: length });
    }

    let mut payload = vec![0_u8; length];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(ClientError::ReadResponse)?;
    Ok(payload)
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("connect to mindcanaryd failed")]
    Connect(#[source] std::io::Error),
    #[error("encode local request failed")]
    EncodeRequest(#[source] serde_json::Error),
    #[error("local request frame length {bytes} is invalid")]
    InvalidRequestLength { bytes: usize },
    #[error("write local request failed")]
    WriteRequest(#[source] std::io::Error),
    #[error("read local response failed")]
    ReadResponse(#[source] std::io::Error),
    #[error("local response frame length {bytes} is invalid")]
    InvalidResponseLength { bytes: usize },
    #[error("decode local response failed")]
    DecodeResponse(#[source] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use mindcanary_protocol::{
        AnnotationRecord, ContextTag, IngestDisposition, ProtocolResponse, RhythmInsightDimension,
        RhythmReadinessStatus, ServiceStatus, SourceHealth, SourceType,
    };
    use uuid::Uuid;

    use super::*;

    #[test]
    fn default_socket_path_matches_daemon() {
        assert_eq!(default_socket_path(), mindcanaryd::default_socket_path());
    }

    #[tokio::test]
    async fn health_round_trip_uses_typed_protocol() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);

        let response = client.health().await.unwrap();

        assert!(matches!(
            response,
            ProtocolResponse::Health {
                status: ServiceStatus::Ready,
                ..
            }
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn source_status_round_trip_uses_typed_protocol() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);

        let _ = client
            .set_signal_collection(SignalId::BrowserTabSwitchCount, true)
            .await
            .unwrap();
        let before = client.source_status().await.unwrap();
        assert_source_status(&before, SourceType::Browser, SourceHealth::NeverSeen, false);

        let request = mindcanary_test_support::synthetic_browser_requests()
            .into_iter()
            .next()
            .unwrap();
        let _ = client.send(&request).await.unwrap();

        let after = client.source_status().await.unwrap();
        assert_source_status(&after, SourceType::Browser, SourceHealth::Active, true);

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    fn assert_source_status(
        response: &ProtocolResponse,
        source: SourceType,
        health: SourceHealth,
        has_received_at: bool,
    ) {
        let ProtocolResponse::SourceStatus { sources, .. } = response else {
            panic!("expected source status");
        };
        let status = sources
            .iter()
            .find(|status| status.source == source)
            .expect("source status should be present");
        assert_eq!(status.health, health);
        assert_eq!(status.last_received_at.is_some(), has_received_at);
    }

    #[tokio::test]
    async fn collection_setting_round_trip_uses_typed_protocol() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);

        let response = client
            .set_signal_collection(SignalId::BrowserTabSwitchCount, false)
            .await
            .unwrap();
        let ProtocolResponse::CollectionSettings { settings, .. } = response else {
            panic!("expected collection settings");
        };
        let setting = settings
            .iter()
            .find(|setting| setting.signal == SignalId::BrowserTabSwitchCount)
            .unwrap();
        assert!(!setting.enabled);
        assert!(setting.changed_at.is_some());

        let reread = client.collection_settings().await.unwrap();
        assert!(matches!(
            reread,
            ProtocolResponse::CollectionSettings { .. }
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn platform_capabilities_round_trip_uses_typed_protocol() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);

        let response = client.platform_capabilities().await.unwrap();

        let ProtocolResponse::PlatformCapabilities { capabilities, .. } = response else {
            panic!("expected platform capabilities");
        };
        assert!(!capabilities.capabilities.is_empty());
        assert!(
            capabilities
                .capabilities
                .iter()
                .all(|capability| capability.status
                    != mindcanary_protocol::PlatformCapabilityStatus::Available)
        );

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn check_in_helper_round_trip_uses_typed_protocol() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);
        let request = mindcanary_test_support::synthetic_check_in_requests()
            .into_iter()
            .next()
            .unwrap();
        let ProtocolRequest::SubmitCheckIn { check_in, .. } = request else {
            panic!("expected a synthetic check-in");
        };
        let check_in_id = check_in.check_in_id;

        let response = client.submit_check_in(check_in).await.unwrap();
        assert!(matches!(
            response,
            ProtocolResponse::CheckInAcknowledged {
                check_in_id: received_id,
                disposition: IngestDisposition::Stored,
                ..
            } if received_id == check_in_id
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn annotation_helpers_round_trip_through_socket() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);
        let annotation_id = Uuid::now_v7();
        let annotation = AnnotationRecord {
            annotation_id,
            created_at: chrono::Utc::now(),
            time_zone: "UTC".to_owned(),
            local_date: "2026-06-19".to_owned(),
            start_minute: Some(9 * 60),
            end_minute: Some(10 * 60 + 30),
            text: "Worked away from the usual desk.".to_owned(),
            context_tags: vec![ContextTag::Other],
        };

        assert!(matches!(
            client.save_annotation(annotation).await.unwrap(),
            ProtocolResponse::AnnotationSaved {
                annotation_id: received_id,
                ..
            } if received_id == annotation_id
        ));

        let prepared = client
            .prepare_delete_annotation(annotation_id)
            .await
            .unwrap();
        let ProtocolResponse::DeleteAnnotationConfirmation {
            confirmation_token, ..
        } = prepared
        else {
            panic!("expected annotation deletion confirmation");
        };
        assert!(matches!(
            client
                .delete_annotation(annotation_id, confirmation_token)
                .await
                .unwrap(),
            ProtocolResponse::AnnotationDeleted {
                annotation_id: received_id,
                ..
            } if received_id == annotation_id
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn daily_rhythm_insights_round_trip_through_socket() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);

        let mut requests = mindcanary_test_support::synthetic_browser_requests();
        requests.extend(mindcanary_test_support::synthetic_check_in_requests());
        for request in requests {
            let response = client.send(&request).await.unwrap();
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

        let response = client.daily_rhythm_insights(Some(10)).await.unwrap();
        let ProtocolResponse::DailyRhythmInsights {
            summary,
            readiness,
            insights,
            ..
        } = response
        else {
            panic!("expected daily rhythm insights");
        };

        assert_eq!(summary.daily_snapshot_count, 5);
        assert!(insights.len() <= 10);
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

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn daily_timeline_round_trip_through_socket() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);

        let mut requests = mindcanary_test_support::synthetic_browser_requests();
        requests.extend(mindcanary_test_support::synthetic_check_in_requests());
        for request in requests {
            let _ = client.send(&request).await.unwrap();
        }

        let response = client.daily_timeline(Some(5)).await.unwrap();
        let ProtocolResponse::DailyTimeline { summary, days, .. } = response else {
            panic!("expected daily timeline");
        };
        assert_eq!(summary.returned_day_count, 5);
        assert_eq!(summary.browser_day_count, 5);
        assert_eq!(summary.check_in_day_count, 5);
        assert_eq!(summary.missing_day_count, 0);
        assert_eq!(days.len(), 5);
        assert!(days.iter().any(|day| {
            day.browser
                .as_ref()
                .is_some_and(|browser| browser.open_tab_count_mean.is_some())
        }));
        assert!(days.iter().any(|day| {
            day.check_in
                .as_ref()
                .is_some_and(|check_in| !check_in.context_tags.is_empty())
        }));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn confirmed_clear_round_trip_removes_canonical_records() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);

        let mut requests = mindcanary_test_support::synthetic_browser_requests();
        requests.extend(mindcanary_test_support::synthetic_check_in_requests());
        for request in requests {
            let _ = client.send(&request).await.unwrap();
        }

        let summary = client.local_data_summary().await.unwrap();
        assert!(matches!(
            summary,
            ProtocolResponse::LocalDataSummary {
                summary: mindcanary_protocol::LocalDataSummary {
                    aggregate_batch_count: 20,
                    check_in_count: 5,
                    ..
                },
                ..
            }
        ));

        let prepared = client.prepare_clear_local_records().await.unwrap();
        let ProtocolResponse::ClearLocalRecordsConfirmation {
            confirmation_token,
            summary,
            ..
        } = prepared
        else {
            panic!("expected clear confirmation");
        };
        assert_eq!(summary.aggregate_batch_count, 20);
        assert_eq!(summary.check_in_count, 5);

        let cleared = client
            .clear_local_records(confirmation_token)
            .await
            .unwrap();
        assert!(matches!(
            cleared,
            ProtocolResponse::LocalRecordsCleared {
                deleted: mindcanary_protocol::LocalDataSummary {
                    aggregate_batch_count: 20,
                    check_in_count: 5,
                    ..
                },
                ..
            }
        ));

        let empty = client.local_data_summary().await.unwrap();
        assert!(matches!(
            empty,
            ProtocolResponse::LocalDataSummary {
                summary: mindcanary_protocol::LocalDataSummary {
                    aggregate_batch_count: 0,
                    aggregate_metric_count: 0,
                    check_in_count: 0,
                    context_tag_count: 0,
                    ..
                },
                ..
            }
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn confirmed_export_round_trip_writes_local_files_without_clearing_records() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);
        let export_dir = tempfile::TempDir::new().unwrap();

        let mut requests = mindcanary_test_support::synthetic_browser_requests();
        requests.extend(mindcanary_test_support::synthetic_check_in_requests());
        for request in requests {
            let _ = client.send(&request).await.unwrap();
        }

        let prepared = client.prepare_export_local_records().await.unwrap();
        let ProtocolResponse::ExportLocalRecordsConfirmation {
            confirmation_token,
            summary,
            ..
        } = prepared
        else {
            panic!("expected export confirmation");
        };
        assert_eq!(summary.aggregate_batch_count, 20);
        assert_eq!(summary.check_in_count, 5);

        let exported = client
            .export_local_records(confirmation_token, export_dir.path().display().to_string())
            .await
            .unwrap();
        let ProtocolResponse::LocalRecordsExported { export, .. } = exported else {
            panic!("expected export response");
        };
        assert_eq!(export.summary.aggregate_batch_count, 20);
        assert_eq!(export.summary.check_in_count, 5);
        assert!(export.report_path.ends_with("mindcanary-report.md"));
        assert!(std::fs::metadata(export.report_path).unwrap().is_file());
        assert!(
            std::fs::metadata(export.daily_browser_csv_path)
                .unwrap()
                .is_file()
        );
        assert!(
            std::fs::metadata(export.daily_check_in_csv_path)
                .unwrap()
                .is_file()
        );

        let still_present = client.local_data_summary().await.unwrap();
        assert!(matches!(
            still_present,
            ProtocolResponse::LocalDataSummary {
                summary: mindcanary_protocol::LocalDataSummary {
                    aggregate_batch_count: 20,
                    check_in_count: 5,
                    ..
                },
                ..
            }
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    #[tokio::test]
    async fn confirmed_signal_deletion_removes_only_the_selected_metric() {
        let (socket_path, server) = spawn_test_daemon().await;
        let client = DaemonClient::new(&socket_path);

        for request in mindcanary_test_support::synthetic_browser_requests() {
            let _ = client.send(&request).await.unwrap();
        }

        let prepared = client
            .prepare_delete_signal_records(SignalId::BrowserTabSwitchCount)
            .await
            .unwrap();
        let ProtocolResponse::DeleteSignalRecordsConfirmation {
            confirmation_token,
            summary,
            ..
        } = prepared
        else {
            panic!("expected signal deletion confirmation");
        };
        assert!(summary.metric_record_count > 0);

        let deleted = client
            .delete_signal_records(SignalId::BrowserTabSwitchCount, confirmation_token)
            .await
            .unwrap();
        assert!(matches!(
            deleted,
            ProtocolResponse::SignalRecordsDeleted {
                signal: SignalId::BrowserTabSwitchCount,
                deleted: mindcanary_protocol::SignalRecordSummary {
                    metric_record_count,
                    ..
                },
                ..
            } if metric_record_count > 0
        ));

        let reread = client
            .prepare_delete_signal_records(SignalId::BrowserTabSwitchCount)
            .await
            .unwrap();
        assert!(matches!(
            reread,
            ProtocolResponse::DeleteSignalRecordsConfirmation {
                summary: mindcanary_protocol::SignalRecordSummary {
                    metric_record_count: 0,
                    affected_batch_count: 0,
                },
                ..
            }
        ));
        let other = client
            .prepare_delete_signal_records(SignalId::BrowserOpenTabCountMean)
            .await
            .unwrap();
        assert!(matches!(
            other,
            ProtocolResponse::DeleteSignalRecordsConfirmation {
                summary: mindcanary_protocol::SignalRecordSummary {
                    metric_record_count,
                    ..
                },
                ..
            } if metric_record_count > 0
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(socket_path.parent().unwrap());
    }

    async fn spawn_test_daemon() -> (PathBuf, tokio::task::JoinHandle<()>) {
        let runtime_dir =
            std::env::temp_dir().join(format!("mindcanary-client-test-{}", Uuid::now_v7()));
        let socket_path = runtime_dir.join("mindcanaryd.sock");
        let server_path = socket_path.clone();
        let data_dir = tempfile::TempDir::new().unwrap();
        let key = mindcanary_storage::DatabaseKey::from_bytes([23; 32]);
        let mut store =
            mindcanary_storage::EncryptedStore::open(data_dir.path().join("mindcanary.db"), &key)
                .unwrap();
        let enabled_at = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        for signal in SignalId::ALL {
            store
                .set_signal_collection(signal, true, enabled_at)
                .unwrap();
        }
        let state = std::sync::Arc::new(mindcanaryd::DaemonState::new(store));
        let server = tokio::spawn(async move {
            let _ = mindcanaryd::run_with_state(&server_path, state).await;
        });

        for _ in 0..100 {
            if socket_path.exists() {
                return (socket_path, server);
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        panic!("daemon socket was not created");
    }

    fn assert_neutral_language(insights: &[mindcanary_protocol::RhythmInsight]) {
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
