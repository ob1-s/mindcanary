use std::{
    io::{stdin, stdout},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use mindcanary_native_host::{
    ExtensionChannel, NATIVE_HOST_NAME, NativeHostManifestHealth, NativeMessagingBrowser,
    configured_browser_extension_id, error_response, forward_request, inspect_native_host_manifest,
    install_native_host_manifest, parse_request, read_chrome_message,
    uninstall_native_host_manifest, write_chrome_message,
};
use mindcanary_protocol::ErrorCode;

#[derive(Debug, Parser)]
#[command(
    name = "mindcanary-native-host",
    about = "Browser native-messaging bridge for MindCanary"
)]
struct Arguments {
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Install a user-level browser native-messaging manifest instead of running the bridge.
    #[arg(long)]
    install_manifest: bool,

    /// Remove the user-level Chrome native-messaging manifest instead of running the bridge.
    #[arg(long)]
    uninstall_manifest: bool,

    /// Print missing, ready, or `needs_repair` for the expected native-host manifest.
    #[arg(long)]
    check_manifest: bool,

    /// Browser registration target for --install-manifest.
    #[arg(long, value_enum, default_value_t = BrowserArgument::Chrome)]
    browser: BrowserArgument,

    /// Build identity allowed to launch this native host.
    #[arg(long, value_enum)]
    channel: Option<ChannelArgument>,

    /// Absolute executable path written to the native-host manifest.
    #[arg(long)]
    host_path: Option<PathBuf>,

    /// Override the manifest directory. Intended for development and tests.
    #[arg(long, hide = true)]
    manifest_dir: Option<PathBuf>,

    /// Caller origin supplied by Chrome.
    origin: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BrowserArgument {
    Chrome,
    Chromium,
    Firefox,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ChannelArgument {
    Development,
    Release,
}

impl From<BrowserArgument> for NativeMessagingBrowser {
    fn from(value: BrowserArgument) -> Self {
        match value {
            BrowserArgument::Chrome => Self::Chrome,
            BrowserArgument::Chromium => Self::Chromium,
            BrowserArgument::Firefox => Self::Firefox,
        }
    }
}

impl From<ChannelArgument> for ExtensionChannel {
    fn from(value: ChannelArgument) -> Self {
        match value {
            ChannelArgument::Development => Self::Development,
            ChannelArgument::Release => Self::Release,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let arguments = Arguments::parse();
    anyhow::ensure!(
        [
            arguments.install_manifest,
            arguments.uninstall_manifest,
            arguments.check_manifest
        ]
        .into_iter()
        .filter(|selected| *selected)
        .count()
            <= 1,
        "manifest operations cannot be used together"
    );
    if arguments.install_manifest || arguments.check_manifest {
        let channel = arguments
            .channel
            .context("--channel is required with manifest operations")?;
        let browser = NativeMessagingBrowser::from(arguments.browser);
        let extension_id = configured_browser_extension_id(browser, channel.into())?;
        let host_path = match arguments.host_path {
            Some(path) => path,
            None => std::env::current_exe().context("resolve current executable path")?,
        };
        if arguments.check_manifest {
            let status = inspect_native_host_manifest(
                browser,
                channel.into(),
                &host_path,
                arguments.manifest_dir.as_deref(),
            )?;
            println!(
                "{}",
                match status.health {
                    NativeHostManifestHealth::Missing => "missing",
                    NativeHostManifestHealth::Ready => "ready",
                    NativeHostManifestHealth::NeedsRepair => "needs_repair",
                }
            );
            return Ok(());
        }
        let manifest_path = install_native_host_manifest(
            browser,
            &extension_id,
            &host_path,
            arguments.manifest_dir.as_deref(),
        )?;
        println!(
            "Installed {NATIVE_HOST_NAME} native-host manifest at {}",
            manifest_path.display()
        );
        return Ok(());
    }

    if arguments.uninstall_manifest {
        let manifest_path = uninstall_native_host_manifest(
            arguments.browser.into(),
            arguments.manifest_dir.as_deref(),
        )?;
        println!(
            "Removed {NATIVE_HOST_NAME} native-host manifest at {}",
            manifest_path.display()
        );
        println!("This does not remove browser extension storage or pending extension queues.");
        return Ok(());
    }

    let socket_path = arguments
        .socket
        .unwrap_or_else(mindcanary_client::default_socket_path);
    let mut input = stdin().lock();
    let mut output = stdout().lock();

    let Ok(payload) = read_chrome_message(&mut input) else {
        write_chrome_message(&mut output, &error_response(ErrorCode::MessageTooLarge)?)?;
        return Ok(());
    };

    let Ok(request) = parse_request(&payload) else {
        write_chrome_message(&mut output, &error_response(ErrorCode::InvalidRequest)?)?;
        return Ok(());
    };

    let response = match forward_request(&socket_path, &request).await {
        Ok(response) => response,
        Err(_) => error_response(ErrorCode::Internal)?,
    };

    write_chrome_message(&mut output, &response)
}
