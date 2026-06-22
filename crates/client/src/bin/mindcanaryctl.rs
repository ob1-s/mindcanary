use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mindcanary_client::{DaemonClient, default_socket_path};
use mindcanary_protocol::{
    LocalDataSummary, PROTOCOL_VERSION, ProtocolResponse, ServiceStatus, SignalCollectionSetting,
    SignalId, SourceHealth, SourceStatus, SourceType,
};

#[derive(Debug, Parser)]
#[command(
    name = "mindcanaryctl",
    about = "Small local control CLI for the MindCanary daemon"
)]
struct Arguments {
    /// Override the daemon Unix socket path.
    #[arg(long)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check daemon health and protocol compatibility.
    Health,
    /// Show whether each local source is active, stale, disabled, or unavailable.
    SourceStatus,
    /// List known aggregate signals.
    Signals,
    /// Show current per-signal collection settings.
    Settings,
    /// Enable collection for one aggregate signal.
    Enable {
        #[arg(value_parser = parse_signal)]
        signal: SignalId,
    },
    /// Pause collection for one aggregate signal.
    Disable {
        #[arg(value_parser = parse_signal)]
        signal: SignalId,
    },
    /// Enable the minimal browser aggregate set for local smoke testing.
    EnableBrowserDefaults,
    /// Pause the default browser aggregate set.
    DisableBrowserDefaults,
    /// Show counts of local canonical records.
    Summary,
    /// Export a local report and daily aggregate CSVs without clearing records.
    Export {
        /// Absolute directory where export files should be written.
        #[arg(long)]
        directory: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments = Arguments::parse();
    let client = DaemonClient::new(arguments.socket.unwrap_or_else(default_socket_path));

    match arguments.command {
        Command::Health => print_health(client.health().await.context("request daemon health")?)?,
        Command::SourceStatus => print_source_status(
            client
                .source_status()
                .await
                .context("request source status")?,
        )?,
        Command::Signals => print_signals(),
        Command::Settings => {
            print_settings(
                client
                    .collection_settings()
                    .await
                    .context("request settings")?,
            )?;
        }
        Command::Enable { signal } => {
            let response = client
                .set_signal_collection(signal, true)
                .await
                .with_context(|| format!("enable {}", signal.as_str()))?;
            print_changed_setting(response, signal)?;
        }
        Command::Disable { signal } => {
            let response = client
                .set_signal_collection(signal, false)
                .await
                .with_context(|| format!("disable {}", signal.as_str()))?;
            print_changed_setting(response, signal)?;
        }
        Command::EnableBrowserDefaults => {
            set_signal_group(&client, &SignalId::BROWSER_STARTER_SET, true).await?;
        }
        Command::DisableBrowserDefaults => {
            set_signal_group(&client, &SignalId::BROWSER_STARTER_SET, false).await?;
        }
        Command::Summary => {
            print_summary(
                &client
                    .local_data_summary()
                    .await
                    .context("request local data summary")?,
            )?;
        }
        Command::Export { directory } => {
            export_local_records(&client, &directory).await?;
        }
    }

    Ok(())
}

async fn export_local_records(client: &DaemonClient, directory: &Path) -> Result<()> {
    let export_directory = export_directory_argument(directory)?;
    let prepared = client
        .prepare_export_local_records()
        .await
        .context("prepare local records export")?;
    let ProtocolResponse::ExportLocalRecordsConfirmation {
        confirmation_token, ..
    } = prepared
    else {
        bail!("daemon returned an unexpected export confirmation response");
    };

    let exported = client
        .export_local_records(confirmation_token, export_directory)
        .await
        .context("export local records")?;
    print_exported_records(&exported)
}

fn export_directory_argument(directory: &Path) -> Result<String> {
    if !directory.is_absolute() {
        bail!("export directory must be absolute");
    }
    Ok(directory.display().to_string())
}

async fn set_signal_group(
    client: &DaemonClient,
    signals: &[SignalId],
    enabled: bool,
) -> Result<()> {
    for signal in signals {
        let response = client
            .set_signal_collection(*signal, enabled)
            .await
            .with_context(|| format!("set {}", signal.as_str()))?;
        print_changed_setting(response, *signal)?;
    }
    Ok(())
}

fn parse_signal(value: &str) -> std::result::Result<SignalId, String> {
    SignalId::from_wire(value).ok_or_else(|| {
        let known = SignalId::ALL
            .into_iter()
            .map(SignalId::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        format!("unknown signal {value:?}; known signals: {known}")
    })
}

fn print_health(response: ProtocolResponse) -> Result<()> {
    let ProtocolResponse::Health {
        protocol_version,
        service_version,
        status,
    } = response
    else {
        bail!("daemon returned an unexpected health response");
    };

    println!("MindCanary daemon: {}", service_status_label(status));
    println!("Service version: {service_version}");
    println!("Protocol: {protocol_version} local / {PROTOCOL_VERSION} client");
    Ok(())
}

fn print_source_status(response: ProtocolResponse) -> Result<()> {
    let ProtocolResponse::SourceStatus { sources, .. } = response else {
        bail!("daemon returned an unexpected source status response");
    };

    for source in sources {
        println!("{}", source_status_line(source));
    }
    Ok(())
}

fn source_status_line(status: SourceStatus) -> String {
    let source = match status.source {
        SourceType::Browser => "browser",
        SourceType::Os => "os",
        SourceType::CheckIn => "check-in",
    };
    let health = match status.health {
        SourceHealth::NeverSeen => "never_seen",
        SourceHealth::Active => "active",
        SourceHealth::Stale => "stale",
        SourceHealth::Disabled => "disabled",
        SourceHealth::Unavailable => "unavailable",
    };
    let received = status
        .last_received_at
        .map_or_else(|| "never".to_owned(), |value| value.to_rfc3339());
    format!("{source}: {health} (last received: {received})")
}

fn print_signals() {
    for signal in SignalId::ALL {
        println!("{}", signal.as_str());
    }
}

fn print_settings(response: ProtocolResponse) -> Result<()> {
    let ProtocolResponse::CollectionSettings { settings, .. } = response else {
        bail!("daemon returned an unexpected settings response");
    };

    for setting in settings {
        println!("{}", setting_line(&setting));
    }
    Ok(())
}

fn print_changed_setting(response: ProtocolResponse, signal: SignalId) -> Result<()> {
    let ProtocolResponse::CollectionSettings { settings, .. } = response else {
        bail!("daemon returned an unexpected settings response");
    };
    let setting = settings
        .iter()
        .find(|setting| setting.signal == signal)
        .context("daemon response did not include the requested signal")?;

    println!("{}", setting_line(setting));
    Ok(())
}

fn setting_line(setting: &SignalCollectionSetting) -> String {
    let state = if setting.enabled { "enabled" } else { "paused" };
    let changed_at = setting
        .changed_at
        .map_or_else(|| "never changed".to_owned(), |value| value.to_rfc3339());
    format!("{}: {state} ({changed_at})", setting.signal.as_str())
}

fn print_summary(response: &ProtocolResponse) -> Result<()> {
    let ProtocolResponse::LocalDataSummary { summary, .. } = response else {
        bail!("daemon returned an unexpected summary response");
    };
    print_local_data_summary(*summary);
    Ok(())
}

fn print_exported_records(response: &ProtocolResponse) -> Result<()> {
    let ProtocolResponse::LocalRecordsExported { export, .. } = response else {
        bail!("daemon returned an unexpected export response");
    };

    println!(
        "Exported local MindCanary records to {}",
        export.export_directory
    );
    print_local_data_summary(export.summary);
    println!("Report: {}", export.report_path);
    println!("Browser CSV: {}", export.daily_browser_csv_path);
    println!("OS CSV: {}", export.daily_os_csv_path);
    println!("Check-in CSV: {}", export.daily_check_in_csv_path);
    if !export.annotations_csv_path.is_empty() {
        println!("Annotations CSV: {}", export.annotations_csv_path);
    }
    Ok(())
}

fn print_local_data_summary(summary: LocalDataSummary) {
    println!("Aggregate batches: {}", summary.aggregate_batch_count);
    println!("Aggregate metrics: {}", summary.aggregate_metric_count);
    println!("Check-ins: {}", summary.check_in_count);
    println!("Context tags: {}", summary.context_tag_count);
    println!("Annotations: {}", summary.annotation_count);
    println!(
        "Annotation context tags: {}",
        summary.annotation_context_tag_count
    );
}

const fn service_status_label(status: ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::Ready => "ready",
        ServiceStatus::Degraded => "degraded",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_signals() {
        assert_eq!(
            parse_signal("browser.open_tab_count_mean"),
            Ok(SignalId::BrowserOpenTabCountMean)
        );
        assert_eq!(
            parse_signal("os.active_seconds"),
            Ok(SignalId::OsActiveSeconds)
        );
    }

    #[test]
    fn rejects_unknown_signals_with_known_names() {
        let error = parse_signal("browser.url").unwrap_err();

        assert!(error.contains("unknown signal"));
        assert!(error.contains("browser.open_tab_count_mean"));
        assert!(!error.contains("title"));
    }

    #[test]
    fn browser_defaults_are_privacy_preserving_aggregates() {
        assert_eq!(
            SignalId::BROWSER_STARTER_SET,
            [
                SignalId::BrowserTabSwitchCount,
                SignalId::BrowserOpenTabCountMax,
                SignalId::BrowserOpenTabCountMean,
                SignalId::BrowserWindowCountMax,
                SignalId::BrowserActiveSeconds,
                SignalId::BrowserIdleSeconds,
                SignalId::BrowserRetainedAcrossDayCount,
            ]
        );
        let names = SignalId::BROWSER_STARTER_SET
            .iter()
            .map(|signal| signal.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!names.contains("url"));
        assert!(!names.contains("title"));
        assert!(!names.contains("history"));
    }

    #[test]
    fn source_status_output_is_narrow_and_human_readable() {
        let line = source_status_line(SourceStatus {
            source: SourceType::Browser,
            health: SourceHealth::Active,
            last_received_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-06-15T12:34:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
        });

        assert_eq!(
            line,
            "browser: active (last received: 2026-06-15T12:34:00+00:00)"
        );
        assert!(!line.contains("url"));
        assert!(!line.contains("title"));
    }

    #[test]
    fn export_requires_absolute_directory() {
        let error = export_directory_argument(Path::new("relative-export")).unwrap_err();
        assert!(error.to_string().contains("must be absolute"));

        assert_eq!(
            export_directory_argument(Path::new("/tmp/mindcanary-export")).unwrap(),
            "/tmp/mindcanary-export"
        );
    }
}
