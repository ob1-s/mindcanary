use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::Serialize;

const PACKAGED_DAEMON_PATH: &str = "/usr/lib/mindcanary/mindcanaryd";
const PACKAGED_NATIVE_HOST_PATH: &str = "/usr/lib/mindcanary/mindcanary-native-host";
const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
const USER_SERVICE_NAME: &str = "mindcanaryd.service";
const USER_SERVICE_SESSION_ENVIRONMENT: &[&str] = &[
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "DESKTOP_SESSION",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "XAUTHORITY",
];
pub const LOCAL_REMOVAL_CONFIRMATION_PHRASE: &str = "DELETE LOCAL MINDCANARY DATA";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChromeConnectorRuntime {
    Development,
    Packaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChromeConnectorHealth {
    Missing,
    Ready,
    NeedsRepair,
    HelperMissing,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChromeConnectorStatus {
    pub runtime: ChromeConnectorRuntime,
    pub health: ChromeConnectorHealth,
    pub setup_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalRemovalReport {
    pub user_service_removed: bool,
    pub native_host_manifests_removed: Vec<String>,
    pub database_profile_destroyed: bool,
    pub package_marker_removed: bool,
    pub runtime_socket_dir_removed: bool,
    pub browser_extension_storage_removed: bool,
    pub user_exports_removed: bool,
}

pub fn ensure_packaged_daemon_service() -> Result<()> {
    let marker_path = package_version_marker_path()?;
    import_user_service_session_environment();
    if daemon_is_ready() && marker_matches(&marker_path, PACKAGE_VERSION) {
        if daemon_service_needs_session_environment_refresh() {
            restart_user_service()?;
            wait_for_daemon_socket(Duration::from_secs(5))?;
        }
        return Ok(());
    }

    install_daemon_service(Path::new(PACKAGED_DAEMON_PATH))?;
    wait_for_daemon_socket(Duration::from_secs(5))?;
    write_version_marker(&marker_path, PACKAGE_VERSION)?;
    Ok(())
}

pub fn chrome_connector_status() -> ChromeConnectorStatus {
    chrome_connector_status_for(current_runtime(), native_host_path())
}

pub fn connect_chrome() -> Result<ChromeConnectorStatus> {
    let runtime = current_runtime();
    let host_path = native_host_path();
    if runtime == ChromeConnectorRuntime::Packaged {
        install_native_host_manifest_with(&host_path, configured_channel())?;
    }
    Ok(chrome_connector_status_for(runtime, host_path))
}

pub fn complete_local_removal() -> Result<LocalRemovalReport> {
    let daemon_path = daemon_path();
    let native_host_path = native_host_path();
    validate_executable(&daemon_path, "daemon")?;
    validate_executable(&native_host_path, "native host")?;

    stop_user_service_if_available();
    uninstall_daemon_service_with(&daemon_path)?;
    let native_host_manifests_removed = uninstall_native_host_manifests_with(&native_host_path)?;
    destroy_local_profile_with(&daemon_path)?;
    let package_marker_removed = remove_package_version_marker()?;
    let runtime_socket_dir_removed = remove_runtime_socket_dir()?;

    Ok(LocalRemovalReport {
        user_service_removed: true,
        native_host_manifests_removed,
        database_profile_destroyed: true,
        package_marker_removed,
        runtime_socket_dir_removed,
        browser_extension_storage_removed: false,
        user_exports_removed: false,
    })
}

fn chrome_connector_status_for(
    runtime: ChromeConnectorRuntime,
    host_path: PathBuf,
) -> ChromeConnectorStatus {
    let channel = configured_channel();
    let setup_command = (runtime == ChromeConnectorRuntime::Development)
        .then(|| development_setup_command(channel));

    if validate_executable(&host_path, "native host").is_err() {
        return ChromeConnectorStatus {
            runtime,
            health: ChromeConnectorHealth::HelperMissing,
            setup_command,
        };
    }

    let health = match native_host_manifest_status(&host_path, channel) {
        Ok("missing") => ChromeConnectorHealth::Missing,
        Ok("ready") => ChromeConnectorHealth::Ready,
        Ok("needs_repair") => ChromeConnectorHealth::NeedsRepair,
        Ok(_) | Err(_) => ChromeConnectorHealth::Unavailable,
    };

    ChromeConnectorStatus {
        runtime,
        health,
        setup_command,
    }
}

fn install_native_host_manifest_with(native_host_path: &Path, channel: &'static str) -> Result<()> {
    let status = native_host_manifest_install_command(native_host_path, channel)?
        .status()
        .context("run packaged MindCanary native-host installer")?;
    anyhow::ensure!(
        status.success(),
        "packaged MindCanary native-host installer failed"
    );
    Ok(())
}

fn native_host_manifest_install_command(
    native_host_path: &Path,
    channel: &'static str,
) -> Result<Command> {
    validate_executable(native_host_path, "native host")?;
    let mut command = Command::new(native_host_path);
    command
        .arg("--install-manifest")
        .arg("--browser")
        .arg("chrome")
        .arg("--channel")
        .arg(channel)
        .arg("--host-path")
        .arg(native_host_path);
    Ok(command)
}

fn native_host_manifest_status(
    native_host_path: &Path,
    channel: &'static str,
) -> Result<&'static str> {
    let output = native_host_manifest_status_command(native_host_path, channel)?
        .output()
        .context("run MindCanary native-host manifest check")?;
    anyhow::ensure!(
        output.status.success(),
        "MindCanary native-host manifest check failed"
    );
    match String::from_utf8(output.stdout)
        .context("native-host manifest status must be UTF-8")?
        .trim()
    {
        "missing" => Ok("missing"),
        "ready" => Ok("ready"),
        "needs_repair" => Ok("needs_repair"),
        _ => anyhow::bail!("native-host manifest check returned an unknown status"),
    }
}

fn native_host_manifest_status_command(
    native_host_path: &Path,
    channel: &'static str,
) -> Result<Command> {
    validate_executable(native_host_path, "native host")?;
    let mut command = Command::new(native_host_path);
    command
        .arg("--check-manifest")
        .arg("--browser")
        .arg("chrome")
        .arg("--channel")
        .arg(channel)
        .arg("--host-path")
        .arg(native_host_path);
    Ok(command)
}

fn native_host_manifest_uninstall_command(
    native_host_path: &Path,
    browser: &'static str,
) -> Result<Command> {
    validate_executable(native_host_path, "native host")?;
    let mut command = Command::new(native_host_path);
    command
        .arg("--uninstall-manifest")
        .arg("--browser")
        .arg(browser);
    Ok(command)
}

fn uninstall_native_host_manifests_with(native_host_path: &Path) -> Result<Vec<String>> {
    let mut removed = Vec::new();
    for browser in ["chrome", "chromium"] {
        let status = native_host_manifest_uninstall_command(native_host_path, browser)?
            .status()
            .with_context(|| format!("run MindCanary {browser} native-host uninstaller"))?;
        anyhow::ensure!(
            status.success(),
            "MindCanary {browser} native-host uninstaller failed"
        );
        removed.push(browser.to_owned());
    }
    Ok(removed)
}

fn current_runtime() -> ChromeConnectorRuntime {
    if cfg!(debug_assertions) {
        ChromeConnectorRuntime::Development
    } else {
        ChromeConnectorRuntime::Packaged
    }
}

fn configured_channel() -> &'static str {
    match option_env!("MINDCANARY_EXTENSION_CHANNEL") {
        Some("release") => "release",
        _ => "development",
    }
}

fn native_host_path() -> PathBuf {
    if current_runtime() == ChromeConnectorRuntime::Packaged {
        return PathBuf::from(PACKAGED_NATIVE_HOST_PATH);
    }
    workspace_root().join("target/debug/mindcanary-native-host")
}

fn daemon_path() -> PathBuf {
    if current_runtime() == ChromeConnectorRuntime::Packaged {
        return PathBuf::from(PACKAGED_DAEMON_PATH);
    }
    workspace_root().join("target/debug/mindcanaryd")
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("desktop crate must remain under apps/desktop/src-tauri")
}

fn development_setup_command(channel: &str) -> String {
    format!(
        "cargo build -p mindcanary-native-host\ncargo run -p mindcanary-native-host -- \\\n  --install-manifest \\\n  --browser chrome \\\n  --channel {} \\\n  --host-path \"$PWD/target/debug/mindcanary-native-host\"",
        channel
    )
}

fn install_daemon_service(daemon_path: &Path) -> Result<()> {
    let status = daemon_service_install_command(daemon_path)?
        .status()
        .context("run packaged MindCanary daemon service installer")?;
    anyhow::ensure!(
        status.success(),
        "packaged MindCanary daemon service installer failed"
    );
    Ok(())
}

fn uninstall_daemon_service_with(daemon_path: &Path) -> Result<()> {
    let status = daemon_service_uninstall_command(daemon_path)?
        .status()
        .context("run packaged MindCanary daemon service uninstaller")?;
    anyhow::ensure!(
        status.success(),
        "packaged MindCanary daemon service uninstaller failed"
    );
    Ok(())
}

fn daemon_service_install_command(daemon_path: &Path) -> Result<Command> {
    validate_executable(daemon_path, "daemon")?;
    let mut command = Command::new(daemon_path);
    command
        .arg("--install-user-service")
        .arg("--daemon-path")
        .arg(daemon_path)
        .arg("--enable-now");
    Ok(command)
}

fn daemon_service_uninstall_command(daemon_path: &Path) -> Result<Command> {
    validate_executable(daemon_path, "daemon")?;
    let mut command = Command::new(daemon_path);
    command.arg("--uninstall-user-service");
    Ok(command)
}

fn destroy_local_profile_with(daemon_path: &Path) -> Result<()> {
    let status = daemon_destroy_local_profile_command(daemon_path)?
        .status()
        .context("run MindCanary local profile destruction")?;
    anyhow::ensure!(
        status.success(),
        "MindCanary local profile destruction failed"
    );
    Ok(())
}

fn daemon_destroy_local_profile_command(daemon_path: &Path) -> Result<Command> {
    validate_executable(daemon_path, "daemon")?;
    let mut command = Command::new(daemon_path);
    command
        .arg("--destroy-local-profile")
        .arg("--confirm-destroy-local-profile");
    Ok(command)
}

fn stop_user_service_if_available() {
    let _ = Command::new("systemctl")
        .arg("--user")
        .arg("disable")
        .arg("--now")
        .arg(USER_SERVICE_NAME)
        .status();
}

fn restart_user_service() -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .arg("restart")
        .arg(USER_SERVICE_NAME)
        .status()
        .context("restart MindCanary user service")?;
    anyhow::ensure!(
        status.success(),
        "MindCanary user service restart failed with status {status}"
    );
    Ok(())
}

fn import_user_service_session_environment() {
    let names = session_environment_variable_names();
    if names.is_empty() {
        return;
    }

    let _ = Command::new("systemctl")
        .arg("--user")
        .arg("import-environment")
        .args(names)
        .status();
}

fn session_environment_variable_names() -> Vec<&'static str> {
    USER_SERVICE_SESSION_ENVIRONMENT
        .iter()
        .copied()
        .filter(|name| env::var_os(name).is_some_and(|value| !value.is_empty()))
        .collect()
}

fn daemon_service_needs_session_environment_refresh() -> bool {
    if !session_environment_variable_names()
        .iter()
        .any(|name| matches!(*name, "XDG_CURRENT_DESKTOP" | "XDG_SESSION_TYPE"))
    {
        return false;
    }

    let Some(pid) = user_service_main_pid() else {
        return false;
    };
    let Ok(environ) = fs::read(format!("/proc/{pid}/environ")) else {
        return false;
    };

    !environ_contains_name(&environ, "XDG_CURRENT_DESKTOP")
        || !environ_contains_name(&environ, "XDG_SESSION_TYPE")
}

fn user_service_main_pid() -> Option<u32> {
    let output = Command::new("systemctl")
        .arg("--user")
        .arg("show")
        .arg(USER_SERVICE_NAME)
        .arg("--property")
        .arg("MainPID")
        .arg("--value")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 0)
}

fn environ_contains_name(environ: &[u8], name: &str) -> bool {
    let prefix = format!("{name}=");
    environ
        .split(|byte| *byte == b'\0')
        .any(|entry| entry.starts_with(prefix.as_bytes()))
}

fn validate_executable(path: &Path, label: &str) -> Result<()> {
    if !path.is_absolute() {
        anyhow::bail!("{label} path must be absolute");
    }

    let metadata = fs::metadata(path).with_context(|| format!("read {label} metadata"))?;
    if !metadata.is_file() {
        anyhow::bail!("{label} path is not a file");
    }
    if metadata.permissions().mode() & 0o111 == 0 {
        anyhow::bail!("{label} is not executable");
    }
    Ok(())
}

fn package_version_marker_path() -> Result<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        })
        .context("HOME or XDG_CONFIG_HOME is required to record package setup")?;
    Ok(config_home
        .join("mindcanary")
        .join("daemon-package-version"))
}

fn marker_matches(path: &Path, expected_version: &str) -> bool {
    fs::read_to_string(path).is_ok_and(|version| version.trim() == expected_version)
}

fn write_version_marker(path: &Path, version: &str) -> Result<()> {
    let parent = path
        .parent()
        .context("package version marker has no parent")?;
    fs::create_dir_all(parent).context("create package version marker directory")?;
    fs::write(path, format!("{version}\n")).context("write package version marker")
}

fn remove_package_version_marker() -> Result<bool> {
    let marker_path = package_version_marker_path()?;
    match fs::remove_file(&marker_path) {
        Ok(()) => {
            if let Some(parent) = marker_path.parent() {
                let _ = fs::remove_dir(parent);
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("remove package setup marker"),
    }
}

fn remove_runtime_socket_dir() -> Result<bool> {
    let Some(parent) = mindcanary_client::default_socket_path()
        .parent()
        .map(Path::to_path_buf)
    else {
        return Ok(false);
    };
    match fs::remove_dir_all(&parent) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("remove MindCanary runtime socket directory"),
    }
}

fn daemon_is_ready() -> bool {
    UnixStream::connect(mindcanary_client::default_socket_path()).is_ok()
}

fn wait_for_daemon_socket(timeout: Duration) -> Result<()> {
    let socket_path = mindcanary_client::default_socket_path();
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if UnixStream::connect(&socket_path).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    anyhow::bail!("packaged daemon did not become ready before the startup timeout")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn daemon_service_installer_uses_only_fixed_arguments() {
        let directory = tempfile::TempDir::new().unwrap();
        let daemon_path = directory.path().join("mindcanaryd");
        fs::write(&daemon_path, b"test executable").unwrap();
        fs::set_permissions(&daemon_path, fs::Permissions::from_mode(0o755)).unwrap();

        let command = daemon_service_install_command(&daemon_path).unwrap();

        assert_eq!(command.get_program(), daemon_path.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                OsStr::new("--install-user-service"),
                OsStr::new("--daemon-path"),
                daemon_path.as_os_str(),
                OsStr::new("--enable-now"),
            ]
        );
    }

    #[test]
    fn daemon_service_uninstaller_uses_only_fixed_arguments() {
        let directory = tempfile::TempDir::new().unwrap();
        let daemon_path = directory.path().join("mindcanaryd");
        fs::write(&daemon_path, b"test executable").unwrap();
        fs::set_permissions(&daemon_path, fs::Permissions::from_mode(0o755)).unwrap();

        let command = daemon_service_uninstall_command(&daemon_path).unwrap();

        assert_eq!(command.get_program(), daemon_path.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [OsStr::new("--uninstall-user-service")]
        );
    }

    #[test]
    fn daemon_profile_destroyer_uses_explicit_confirmation_flag() {
        let directory = tempfile::TempDir::new().unwrap();
        let daemon_path = directory.path().join("mindcanaryd");
        fs::write(&daemon_path, b"test executable").unwrap();
        fs::set_permissions(&daemon_path, fs::Permissions::from_mode(0o755)).unwrap();

        let command = daemon_destroy_local_profile_command(&daemon_path).unwrap();

        assert_eq!(command.get_program(), daemon_path.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                OsStr::new("--destroy-local-profile"),
                OsStr::new("--confirm-destroy-local-profile"),
            ]
        );
    }

    #[test]
    fn rejects_missing_or_non_executable_helpers() {
        let directory = tempfile::TempDir::new().unwrap();
        let missing = directory.path().join("missing");
        assert!(daemon_service_install_command(&missing).is_err());

        let daemon_path = directory.path().join("mindcanaryd");
        fs::write(&daemon_path, b"not executable").unwrap();
        fs::set_permissions(&daemon_path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(daemon_service_install_command(&daemon_path).is_err());
    }

    #[test]
    fn packaged_native_host_installer_uses_only_fixed_arguments() {
        let directory = tempfile::TempDir::new().unwrap();
        let native_host_path = directory.path().join("mindcanary-native-host");
        fs::write(&native_host_path, b"test executable").unwrap();
        fs::set_permissions(&native_host_path, fs::Permissions::from_mode(0o755)).unwrap();

        let command =
            native_host_manifest_install_command(&native_host_path, "development").unwrap();

        assert_eq!(command.get_program(), native_host_path.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                OsStr::new("--install-manifest"),
                OsStr::new("--browser"),
                OsStr::new("chrome"),
                OsStr::new("--channel"),
                OsStr::new("development"),
                OsStr::new("--host-path"),
                native_host_path.as_os_str(),
            ]
        );
    }

    #[test]
    fn packaged_native_host_status_uses_only_fixed_arguments() {
        let directory = tempfile::TempDir::new().unwrap();
        let native_host_path = directory.path().join("mindcanary-native-host");
        fs::write(&native_host_path, b"test executable").unwrap();
        fs::set_permissions(&native_host_path, fs::Permissions::from_mode(0o755)).unwrap();

        let command =
            native_host_manifest_status_command(&native_host_path, "development").unwrap();

        assert_eq!(command.get_program(), native_host_path.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                OsStr::new("--check-manifest"),
                OsStr::new("--browser"),
                OsStr::new("chrome"),
                OsStr::new("--channel"),
                OsStr::new("development"),
                OsStr::new("--host-path"),
                native_host_path.as_os_str(),
            ]
        );
    }

    #[test]
    fn native_host_uninstaller_uses_only_fixed_browser_arguments() {
        let directory = tempfile::TempDir::new().unwrap();
        let native_host_path = directory.path().join("mindcanary-native-host");
        fs::write(&native_host_path, b"test executable").unwrap();
        fs::set_permissions(&native_host_path, fs::Permissions::from_mode(0o755)).unwrap();

        let command = native_host_manifest_uninstall_command(&native_host_path, "chrome").unwrap();

        assert_eq!(command.get_program(), native_host_path.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                OsStr::new("--uninstall-manifest"),
                OsStr::new("--browser"),
                OsStr::new("chrome"),
            ]
        );
    }

    #[test]
    fn connector_status_accepts_only_known_helper_output() {
        let directory = tempfile::TempDir::new().unwrap();
        let native_host_path = directory.path().join("mindcanary-native-host");
        fs::write(&native_host_path, "#!/bin/sh\nprintf 'ready\\n'\n").unwrap();
        fs::set_permissions(&native_host_path, fs::Permissions::from_mode(0o755)).unwrap();

        let status =
            chrome_connector_status_for(ChromeConnectorRuntime::Packaged, native_host_path);

        assert_eq!(status.health, ChromeConnectorHealth::Ready);
        assert!(status.setup_command.is_none());
    }

    #[test]
    fn development_status_exposes_only_the_fixed_setup_command() {
        let directory = tempfile::TempDir::new().unwrap();
        let missing_host = directory.path().join("mindcanary-native-host");

        let status = chrome_connector_status_for(ChromeConnectorRuntime::Development, missing_host);

        assert_eq!(status.health, ChromeConnectorHealth::HelperMissing);
        let command = status.setup_command.unwrap();
        assert!(command.contains("--channel development"));
        assert!(!command.contains("--extension-id"));
        assert!(!command.contains(directory.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn command_does_not_accept_runtime_paths_or_arguments() {
        let directory = tempfile::TempDir::new().unwrap();
        let daemon_path = directory.path().join("mindcanaryd");
        fs::write(&daemon_path, b"test executable").unwrap();
        fs::set_permissions(&daemon_path, fs::Permissions::from_mode(0o755)).unwrap();

        let command = daemon_service_install_command(&daemon_path).unwrap();
        assert_eq!(command.get_program(), daemon_path.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                OsStr::new("--install-user-service"),
                OsStr::new("--daemon-path"),
                daemon_path.as_os_str(),
                OsStr::new("--enable-now"),
            ]
        );
    }

    #[test]
    fn packaged_paths_are_stable_system_paths() {
        assert_eq!(
            Path::new(PACKAGED_DAEMON_PATH),
            Path::new("/usr/lib/mindcanary/mindcanaryd")
        );
        assert!(!PACKAGED_DAEMON_PATH.contains("target/"));
        assert!(!PACKAGED_DAEMON_PATH.contains("/home/"));
        assert_eq!(
            Path::new(PACKAGED_NATIVE_HOST_PATH),
            Path::new("/usr/lib/mindcanary/mindcanary-native-host")
        );
        assert!(!PACKAGED_NATIVE_HOST_PATH.contains("target/"));
        assert!(!PACKAGED_NATIVE_HOST_PATH.contains("/home/"));
    }

    #[test]
    fn version_marker_matches_only_the_current_trimmed_version() {
        let directory = tempfile::TempDir::new().unwrap();
        let marker_path = directory.path().join("daemon-package-version");

        assert!(!marker_matches(&marker_path, "0.1.0"));
        write_version_marker(&marker_path, "0.1.0").unwrap();
        assert!(marker_matches(&marker_path, "0.1.0"));
        assert!(!marker_matches(&marker_path, "0.2.0"));
    }

    #[test]
    fn local_removal_report_never_claims_browser_storage_or_exports_removed() {
        let report = LocalRemovalReport {
            user_service_removed: true,
            native_host_manifests_removed: vec!["chrome".to_owned(), "chromium".to_owned()],
            database_profile_destroyed: true,
            package_marker_removed: true,
            runtime_socket_dir_removed: true,
            browser_extension_storage_removed: false,
            user_exports_removed: false,
        };

        assert!(!report.browser_extension_storage_removed);
        assert!(!report.user_exports_removed);
    }

    #[test]
    fn nul_delimited_environment_lookup_matches_complete_names() {
        let environment = b"XDG_CURRENT_DESKTOP=pop:GNOME\0XDG_SESSION_TYPE=x11\0";

        assert!(environ_contains_name(environment, "XDG_CURRENT_DESKTOP"));
        assert!(environ_contains_name(environment, "XDG_SESSION_TYPE"));
        assert!(!environ_contains_name(environment, "CURRENT_DESKTOP"));
        assert!(!environ_contains_name(environment, "XDG_CURRENT"));
    }
}
