use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use mindcanary_storage::{OsKeyringKeyProvider, destroy_local_profile};
use mindcanaryd::{USER_SERVICE_NAME, install_user_service, uninstall_user_service};

#[derive(Debug, Parser)]
#[command(name = "mindcanaryd", about = "MindCanary local data daemon")]
#[allow(clippy::struct_excessive_bools)]
struct Arguments {
    #[arg(long)]
    socket: Option<PathBuf>,

    #[arg(long)]
    database: Option<PathBuf>,

    /// Install the systemd user service instead of running the daemon.
    #[arg(long)]
    install_user_service: bool,

    /// Absolute executable path written to the systemd user service.
    #[arg(long)]
    daemon_path: Option<PathBuf>,

    /// Enable and start the service after writing the unit file.
    #[arg(long)]
    enable_now: bool,

    /// Remove the systemd user service instead of running the daemon.
    #[arg(long)]
    uninstall_user_service: bool,

    /// Disable and stop the service before removing the unit file.
    #[arg(long)]
    disable_now: bool,

    /// Override the systemd user service directory. Intended for development and tests.
    #[arg(long, hide = true)]
    service_dir: Option<PathBuf>,

    /// Destroy the local encrypted database files and OS-keyring database key instead of running.
    #[arg(long)]
    destroy_local_profile: bool,

    /// Required with --destroy-local-profile.
    #[arg(long)]
    confirm_destroy_local_profile: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments = Arguments::parse();
    anyhow::ensure!(
        [
            arguments.install_user_service,
            arguments.uninstall_user_service,
            arguments.destroy_local_profile
        ]
        .into_iter()
        .filter(|selected| *selected)
        .count()
            <= 1,
        "--install-user-service, --uninstall-user-service, and --destroy-local-profile cannot be combined"
    );
    if arguments.install_user_service {
        let daemon_path = match arguments.daemon_path {
            Some(path) => path,
            None => std::env::current_exe()?,
        };
        let service_path = install_user_service(
            &daemon_path,
            arguments.service_dir.as_deref(),
            arguments.enable_now,
        )?;
        println!(
            "Installed {USER_SERVICE_NAME} systemd user service at {}",
            service_path.display()
        );
        if !arguments.enable_now {
            println!(
                "Run `systemctl --user daemon-reload` and `systemctl --user enable --now {USER_SERVICE_NAME}` to start it."
            );
        }
        return Ok(());
    }

    if arguments.uninstall_user_service {
        let service_path =
            uninstall_user_service(arguments.service_dir.as_deref(), arguments.disable_now)?;
        println!(
            "Removed {USER_SERVICE_NAME} systemd user service from {}",
            service_path.display()
        );
        if !arguments.disable_now {
            println!(
                "Run `systemctl --user disable --now {USER_SERVICE_NAME}` if the old service is still active."
            );
        }
        return Ok(());
    }

    if arguments.destroy_local_profile {
        anyhow::ensure!(
            arguments.confirm_destroy_local_profile,
            "--confirm-destroy-local-profile is required with --destroy-local-profile"
        );
        let database_path = arguments
            .database
            .unwrap_or_else(mindcanary_storage::default_database_path);
        let report = destroy_local_profile(&database_path, &OsKeyringKeyProvider)
            .context("destroy local MindCanary profile")?;
        println!(
            "Destroyed local MindCanary database profile at {}",
            report.database_path.display()
        );
        if report.removed_files.is_empty() {
            println!(
                "No database files were present. The OS-keyring entry was still cleared if it existed."
            );
        } else {
            for path in report.removed_files {
                println!("Removed {}", path.display());
            }
        }
        println!(
            "This does not remove Chrome extension storage, native-host manifests, user-service files, or exports saved elsewhere."
        );
        return Ok(());
    }

    let socket_path = arguments
        .socket
        .unwrap_or_else(mindcanaryd::default_socket_path);
    let database_path = arguments
        .database
        .unwrap_or_else(mindcanary_storage::default_database_path);

    mindcanaryd::run(&socket_path, &database_path).await
}
