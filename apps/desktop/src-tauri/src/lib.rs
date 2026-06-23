mod commands;
mod lifecycle;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::runtime_diagnostics,
            commands::ensure_local_service,
            commands::local_service_autostart_status,
            commands::set_local_service_autostart,
            commands::chrome_connector_status,
            commands::connect_chrome,
            commands::complete_local_removal,
            commands::daemon_health,
            commands::source_status,
            commands::daily_rhythm_insights,
            commands::daily_timeline,
            commands::collection_settings,
            commands::platform_capabilities,
            commands::set_signal_collection,
            commands::prepare_delete_signal_records,
            commands::delete_signal_records,
            commands::submit_check_in,
            commands::prepare_delete_latest_check_in,
            commands::delete_latest_check_in,
            commands::save_annotation,
            commands::prepare_delete_annotation,
            commands::delete_annotation,
            commands::local_data_summary,
            commands::prepare_export_local_records,
            commands::export_local_records,
            commands::prepare_create_local_backup,
            commands::create_local_backup,
            commands::verify_local_backup,
            commands::restore_local_backup,
            commands::prepare_clear_local_records,
            commands::clear_local_records,
        ])
        .run(tauri::generate_context!())
        .expect("MindCanary desktop runtime failed");
}
