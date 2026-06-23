use mindcanary_client::{ClientError, DaemonClient};
use mindcanary_protocol::{AnnotationRecord, CheckInRecord, ProtocolResponse, SignalId};

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub async fn ensure_local_service() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(crate::lifecycle::ensure_packaged_daemon_service)
        .await
        .map_err(|_| "local_service_setup_failed".to_owned())?
        .map_err(|_| "local_service_setup_failed".to_owned())
}

#[tauri::command]
pub async fn local_service_autostart_status()
-> Result<crate::lifecycle::LocalServiceAutostartStatus, String> {
    Ok(crate::lifecycle::local_service_autostart_status())
}

#[tauri::command]
pub async fn set_local_service_autostart(
    enabled: bool,
) -> Result<crate::lifecycle::LocalServiceAutostartStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::lifecycle::set_local_service_autostart(enabled)
    })
    .await
    .map_err(|_| "local_service_autostart_failed".to_owned())?
    .map_err(|_| "local_service_autostart_failed".to_owned())
}

#[tauri::command]
pub async fn chrome_connector_status() -> Result<crate::lifecycle::ChromeConnectorStatus, String> {
    Ok(crate::lifecycle::chrome_connector_status())
}

#[tauri::command]
pub async fn connect_chrome() -> Result<crate::lifecycle::ChromeConnectorStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::lifecycle::connect_chrome)
        .await
        .map_err(|_| "chrome_connector_setup_failed".to_owned())?
        .map_err(|_| "chrome_connector_setup_failed".to_owned())
}

#[tauri::command]
pub async fn complete_local_removal(
    confirmation_phrase: String,
) -> Result<crate::lifecycle::LocalRemovalReport, String> {
    if confirmation_phrase != crate::lifecycle::LOCAL_REMOVAL_CONFIRMATION_PHRASE {
        return Err("invalid_local_removal_confirmation".to_owned());
    }

    tauri::async_runtime::spawn_blocking(crate::lifecycle::complete_local_removal)
        .await
        .map_err(|_| "local_removal_failed".to_owned())?
        .map_err(|_| "local_removal_failed".to_owned())
}

#[tauri::command]
pub async fn daemon_health() -> Result<ProtocolResponse, String> {
    client().health().await.map_err(local_service_error)
}

#[tauri::command]
pub async fn source_status() -> Result<ProtocolResponse, String> {
    client().source_status().await.map_err(local_service_error)
}

#[tauri::command]
pub async fn daily_rhythm_insights(limit: Option<u16>) -> Result<ProtocolResponse, String> {
    client()
        .daily_rhythm_insights(limit)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn daily_timeline(limit: Option<u16>) -> Result<ProtocolResponse, String> {
    client()
        .daily_timeline(limit)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn collection_settings() -> Result<ProtocolResponse, String> {
    client()
        .collection_settings()
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn platform_capabilities() -> Result<ProtocolResponse, String> {
    client()
        .platform_capabilities()
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn set_signal_collection(
    signal: SignalId,
    enabled: bool,
) -> Result<ProtocolResponse, String> {
    client()
        .set_signal_collection(signal, enabled)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn prepare_delete_signal_records(signal: SignalId) -> Result<ProtocolResponse, String> {
    client()
        .prepare_delete_signal_records(signal)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn delete_signal_records(
    signal: SignalId,
    confirmation_token: String,
) -> Result<ProtocolResponse, String> {
    let confirmation_token = uuid::Uuid::parse_str(&confirmation_token)
        .map_err(|_| "invalid_confirmation_token".to_owned())?;
    client()
        .delete_signal_records(signal, confirmation_token)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn submit_check_in(check_in: CheckInRecord) -> Result<ProtocolResponse, String> {
    client()
        .submit_check_in(check_in)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn save_annotation(annotation: AnnotationRecord) -> Result<ProtocolResponse, String> {
    client()
        .save_annotation(annotation)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn prepare_delete_annotation(annotation_id: String) -> Result<ProtocolResponse, String> {
    let annotation_id =
        uuid::Uuid::parse_str(&annotation_id).map_err(|_| "invalid_annotation_id".to_owned())?;
    client()
        .prepare_delete_annotation(annotation_id)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn delete_annotation(
    annotation_id: String,
    confirmation_token: String,
) -> Result<ProtocolResponse, String> {
    let annotation_id =
        uuid::Uuid::parse_str(&annotation_id).map_err(|_| "invalid_annotation_id".to_owned())?;
    let confirmation_token = uuid::Uuid::parse_str(&confirmation_token)
        .map_err(|_| "invalid_confirmation_token".to_owned())?;
    client()
        .delete_annotation(annotation_id, confirmation_token)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn local_data_summary() -> Result<ProtocolResponse, String> {
    client()
        .local_data_summary()
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn prepare_clear_local_records() -> Result<ProtocolResponse, String> {
    client()
        .prepare_clear_local_records()
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn prepare_export_local_records() -> Result<ProtocolResponse, String> {
    client()
        .prepare_export_local_records()
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn prepare_create_local_backup() -> Result<ProtocolResponse, String> {
    client()
        .prepare_create_local_backup()
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn create_local_backup(
    confirmation_token: String,
    backup_path: String,
) -> Result<ProtocolResponse, String> {
    let confirmation_token = uuid::Uuid::parse_str(&confirmation_token)
        .map_err(|_| "invalid_confirmation_token".to_owned())?;
    client()
        .create_local_backup(confirmation_token, backup_path)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn verify_local_backup(
    backup_path: String,
    recovery_secret: String,
) -> Result<ProtocolResponse, String> {
    client()
        .verify_local_backup(backup_path, recovery_secret)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn restore_local_backup(
    backup_path: String,
    recovery_secret: String,
) -> Result<ProtocolResponse, String> {
    client()
        .restore_local_backup(backup_path, recovery_secret)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn export_local_records(
    confirmation_token: String,
    export_directory: String,
) -> Result<ProtocolResponse, String> {
    let confirmation_token = uuid::Uuid::parse_str(&confirmation_token)
        .map_err(|_| "invalid_confirmation_token".to_owned())?;
    client()
        .export_local_records(confirmation_token, export_directory)
        .await
        .map_err(local_service_error)
}

#[tauri::command]
pub async fn clear_local_records(confirmation_token: String) -> Result<ProtocolResponse, String> {
    let confirmation_token = uuid::Uuid::parse_str(&confirmation_token)
        .map_err(|_| "invalid_confirmation_token".to_owned())?;
    client()
        .clear_local_records(confirmation_token)
        .await
        .map_err(local_service_error)
}

fn client() -> DaemonClient {
    DaemonClient::for_default_socket()
}

fn local_service_error(_error: ClientError) -> String {
    "local_service_unavailable".to_owned()
}
