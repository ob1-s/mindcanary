use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use mindcanary_client::send_request;
use mindcanary_protocol::{
    ErrorCode, MAX_FRAME_BYTES, PROTOCOL_VERSION, ProtocolRequest, ProtocolResponse,
};
use serde::{Deserialize, Serialize};

pub const NATIVE_HOST_NAME: &str = "app.mindcanary.collector";
const EXTENSION_IDENTITIES_JSON: &str =
    include_str!("../../../config/chrome-extension-identities.json");
const FIREFOX_EXTENSION_IDENTITIES_JSON: &str =
    include_str!("../../../config/firefox-extension-identities.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionChannel {
    Development,
    Release,
}

#[derive(Debug, Deserialize)]
struct ExtensionIdentityRegistry {
    schema_version: u16,
    development: ExtensionIdentityEntry,
    release: ExtensionIdentityEntry,
}

#[derive(Debug, Deserialize)]
struct ExtensionIdentityEntry {
    extension_id: Option<String>,
    manifest_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FirefoxIdentityRegistry {
    schema_version: u16,
    development: FirefoxIdentityEntry,
    release: FirefoxIdentityEntry,
}

#[derive(Debug, Deserialize)]
struct FirefoxIdentityEntry {
    extension_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMessagingBrowser {
    Chrome,
    Chromium,
    Firefox,
}

impl NativeMessagingBrowser {
    fn config_directory_name(self) -> &'static str {
        match self {
            Self::Chrome => "google-chrome",
            Self::Chromium => "chromium",
            Self::Firefox => "mozilla",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NativeHostManifest {
    Chromium(ChromiumNativeHostManifest),
    Firefox(FirefoxNativeHostManifest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromiumNativeHostManifest {
    name: String,
    description: String,
    path: String,
    #[serde(rename = "type")]
    host_type: String,
    allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirefoxNativeHostManifest {
    name: String,
    description: String,
    path: String,
    #[serde(rename = "type")]
    host_type: String,
    allowed_extensions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeHostManifestHealth {
    Missing,
    Ready,
    NeedsRepair,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHostManifestStatus {
    pub health: NativeHostManifestHealth,
    pub path: PathBuf,
}

pub fn manifest_for_host(
    browser: NativeMessagingBrowser,
    host_path: &Path,
    extension_id: &str,
) -> Result<NativeHostManifest> {
    if !host_path.is_absolute() {
        anyhow::bail!("native host manifest requires an absolute host path");
    }

    let host_path = host_path
        .to_str()
        .context("native host path must be valid UTF-8")?
        .to_owned();

    let name = NATIVE_HOST_NAME.to_owned();
    let description = "MindCanary local-first collector bridge".to_owned();
    let host_type = "stdio".to_owned();
    match browser {
        NativeMessagingBrowser::Chrome | NativeMessagingBrowser::Chromium => {
            Ok(NativeHostManifest::Chromium(ChromiumNativeHostManifest {
                name,
                description,
                path: host_path,
                host_type,
                allowed_origins: vec![chrome_extension_origin(extension_id)?],
            }))
        }
        NativeMessagingBrowser::Firefox => {
            validate_firefox_extension_id(extension_id)?;
            Ok(NativeHostManifest::Firefox(FirefoxNativeHostManifest {
                name,
                description,
                path: host_path,
                host_type,
                allowed_extensions: vec![extension_id.to_owned()],
            }))
        }
    }
}

pub fn chrome_extension_origin(extension_id: &str) -> Result<String> {
    if extension_id.len() != 32
        || !extension_id
            .bytes()
            .all(|byte| (b'a'..=b'p').contains(&byte))
    {
        anyhow::bail!("Chrome extension ID must be 32 lowercase characters from a through p");
    }

    Ok(format!("chrome-extension://{extension_id}/"))
}

pub fn configured_extension_id(channel: ExtensionChannel) -> Result<String> {
    let registry: ExtensionIdentityRegistry =
        serde_json::from_str(EXTENSION_IDENTITIES_JSON).context("parse extension identities")?;
    anyhow::ensure!(
        registry.schema_version == 1,
        "unsupported extension identity schema {}",
        registry.schema_version
    );

    let (entry, channel_name) = match channel {
        ExtensionChannel::Development => (&registry.development, "development"),
        ExtensionChannel::Release => (&registry.release, "release"),
    };
    let extension_id = entry
        .extension_id
        .as_deref()
        .with_context(|| format!("Chrome {channel_name} extension ID is not configured"))?;
    chrome_extension_origin(extension_id)?;

    match channel {
        ExtensionChannel::Development => anyhow::ensure!(
            entry.manifest_key.is_some(),
            "Chrome development identity requires a manifest key"
        ),
        ExtensionChannel::Release => anyhow::ensure!(
            entry.manifest_key.is_none(),
            "Chrome release identity must not include a development manifest key"
        ),
    }

    if registry.development.extension_id == registry.release.extension_id {
        anyhow::bail!("Chrome development and release IDs must be different");
    }

    Ok(extension_id.to_owned())
}

pub fn configured_firefox_extension_id(channel: ExtensionChannel) -> Result<String> {
    let registry: FirefoxIdentityRegistry = serde_json::from_str(FIREFOX_EXTENSION_IDENTITIES_JSON)
        .context("parse Firefox extension identities")?;
    anyhow::ensure!(
        registry.schema_version == 1,
        "unsupported Firefox extension identity schema {}",
        registry.schema_version
    );
    let (entry, channel_name) = match channel {
        ExtensionChannel::Development => (&registry.development, "development"),
        ExtensionChannel::Release => (&registry.release, "release"),
    };
    let extension_id = entry
        .extension_id
        .as_deref()
        .with_context(|| format!("Firefox {channel_name} extension ID is not configured"))?;
    validate_firefox_extension_id(extension_id)?;
    if registry.development.extension_id == registry.release.extension_id {
        anyhow::bail!("Firefox development and release IDs must be different");
    }
    Ok(extension_id.to_owned())
}

pub fn configured_browser_extension_id(
    browser: NativeMessagingBrowser,
    channel: ExtensionChannel,
) -> Result<String> {
    match browser {
        NativeMessagingBrowser::Chrome | NativeMessagingBrowser::Chromium => {
            configured_extension_id(channel)
        }
        NativeMessagingBrowser::Firefox => configured_firefox_extension_id(channel),
    }
}

fn validate_firefox_extension_id(extension_id: &str) -> Result<()> {
    anyhow::ensure!(
        !extension_id.trim().is_empty()
            && extension_id.len() <= 255
            && !extension_id.chars().any(char::is_whitespace),
        "Firefox extension ID must be a non-empty identifier without whitespace"
    );
    Ok(())
}

pub fn user_manifest_dir(browser: NativeMessagingBrowser) -> Result<PathBuf> {
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    user_manifest_dir_from_values(browser, xdg_config_home, home)
}

pub fn user_manifest_dir_from_values(
    browser: NativeMessagingBrowser,
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf> {
    if browser == NativeMessagingBrowser::Firefox {
        return home
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| path.join(".mozilla").join("native-messaging-hosts"))
            .context("HOME is required to install the Firefox native host manifest");
    }
    let config_home = match xdg_config_home.filter(|path| !path.as_os_str().is_empty()) {
        Some(path) => path,
        None => home
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| path.join(".config"))
            .context("HOME or XDG_CONFIG_HOME is required to install the native host manifest")?,
    };

    Ok(config_home
        .join(browser.config_directory_name())
        .join("NativeMessagingHosts"))
}

pub fn install_native_host_manifest(
    browser: NativeMessagingBrowser,
    extension_id: &str,
    host_path: &Path,
    manifest_dir: Option<&Path>,
) -> Result<PathBuf> {
    let manifest = manifest_for_host(browser, host_path, extension_id)?;
    let manifest_dir = resolve_manifest_dir(browser, manifest_dir)?;

    fs::create_dir_all(&manifest_dir).context("create native host manifest directory")?;
    let manifest_path = manifest_path(&manifest_dir);
    let manifest_json =
        serde_json::to_vec_pretty(&manifest).context("serialize native host manifest")?;
    fs::write(&manifest_path, manifest_json).context("write native host manifest")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o644))
            .context("set native host manifest permissions")?;
    }

    Ok(manifest_path)
}

pub fn inspect_native_host_manifest(
    browser: NativeMessagingBrowser,
    channel: ExtensionChannel,
    host_path: &Path,
    manifest_dir: Option<&Path>,
) -> Result<NativeHostManifestStatus> {
    let extension_id = configured_browser_extension_id(browser, channel)?;
    let expected = manifest_for_host(browser, host_path, &extension_id)?;
    let manifest_dir = resolve_manifest_dir(browser, manifest_dir)?;
    let path = manifest_path(&manifest_dir);

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NativeHostManifestStatus {
                health: NativeHostManifestHealth::Missing,
                path,
            });
        }
        Err(error) => return Err(error).context("read native host manifest"),
    };
    let health = serde_json::from_slice::<NativeHostManifest>(&bytes)
        .map(|manifest| {
            if manifest == expected {
                NativeHostManifestHealth::Ready
            } else {
                NativeHostManifestHealth::NeedsRepair
            }
        })
        .unwrap_or(NativeHostManifestHealth::NeedsRepair);

    Ok(NativeHostManifestStatus { health, path })
}

pub fn uninstall_native_host_manifest(
    browser: NativeMessagingBrowser,
    manifest_dir: Option<&Path>,
) -> Result<PathBuf> {
    let manifest_dir = resolve_manifest_dir(browser, manifest_dir)?;
    let manifest_path = manifest_path(&manifest_dir);

    match fs::remove_file(&manifest_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("remove native host manifest"),
    }

    Ok(manifest_path)
}

fn resolve_manifest_dir(
    browser: NativeMessagingBrowser,
    manifest_dir: Option<&Path>,
) -> Result<PathBuf> {
    manifest_dir
        .map(Path::to_path_buf)
        .map_or_else(|| user_manifest_dir(browser), Ok)
}

fn manifest_path(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join(format!("{NATIVE_HOST_NAME}.json"))
}

pub fn read_chrome_message(reader: &mut impl Read) -> Result<Vec<u8>> {
    let mut length_bytes = [0_u8; 4];
    reader
        .read_exact(&mut length_bytes)
        .context("read Chrome message length")?;

    let length = u32::from_ne_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        anyhow::bail!("Chrome message exceeds MindCanary's internal frame limit");
    }

    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .context("read Chrome message payload")?;
    Ok(payload)
}

pub fn write_chrome_message(writer: &mut impl Write, payload: &[u8]) -> Result<()> {
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        anyhow::bail!("invalid Chrome response length");
    }

    let length = u32::try_from(payload.len()).context("Chrome response length exceeds u32")?;
    writer
        .write_all(&length.to_ne_bytes())
        .context("write Chrome message length")?;
    writer
        .write_all(payload)
        .context("write Chrome message payload")?;
    writer.flush().context("flush Chrome response")?;
    Ok(())
}

pub fn parse_request(payload: &[u8]) -> Result<ProtocolRequest> {
    let request =
        serde_json::from_slice::<ProtocolRequest>(payload).context("parse protocol request")?;
    request
        .validate_at(chrono::Utc::now())
        .context("validate protocol request")?;
    if !request.is_collector_request() {
        anyhow::bail!("request is not available to the collector bridge");
    }
    Ok(request)
}

pub async fn forward_request(socket_path: &Path, request: &ProtocolRequest) -> Result<Vec<u8>> {
    let response = send_request(socket_path, request)
        .await
        .context("forward request to mindcanaryd")?;
    serde_json::to_vec(&response).context("serialize daemon response")
}

pub fn error_response(code: ErrorCode) -> Result<Vec<u8>> {
    serde_json::to_vec(&ProtocolResponse::Error {
        protocol_version: PROTOCOL_VERSION,
        code,
    })
    .context("serialize error response")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    #[test]
    fn chrome_framing_uses_native_endian_length() {
        let payload = br#"{"type":"health","protocol_version":1}"#;
        let mut encoded = Vec::new();
        write_chrome_message(&mut encoded, payload).unwrap();

        let mut cursor = Cursor::new(encoded);
        assert_eq!(read_chrome_message(&mut cursor).unwrap(), payload);
    }

    #[test]
    fn oversized_chrome_messages_are_rejected_before_allocation() {
        let encoded = u32::try_from(MAX_FRAME_BYTES + 1)
            .unwrap()
            .to_ne_bytes()
            .to_vec();
        let mut cursor = Cursor::new(encoded);

        assert!(read_chrome_message(&mut cursor).is_err());
    }

    #[test]
    fn parser_rejects_url_fields() {
        let payload = serde_json::to_vec(&json!({
            "type": "health",
            "protocol_version": 1,
            "url": "https://example.com/private"
        }))
        .unwrap();

        assert!(parse_request(&payload).is_err());
    }

    #[test]
    fn parser_rejects_administrative_requests() {
        for request in [
            ProtocolRequest::GetSourceStatus {
                protocol_version: PROTOCOL_VERSION,
            },
            ProtocolRequest::PrepareClearLocalRecords {
                protocol_version: PROTOCOL_VERSION,
            },
        ] {
            let payload = serde_json::to_vec(&request).unwrap();
            assert!(parse_request(&payload).is_err());
        }
    }

    #[test]
    fn parser_allows_read_only_collector_settings() {
        let payload = serde_json::to_vec(&ProtocolRequest::GetCollectionSettings {
            protocol_version: PROTOCOL_VERSION,
        })
        .unwrap();

        assert!(matches!(
            parse_request(&payload).unwrap(),
            ProtocolRequest::GetCollectionSettings { .. }
        ));
    }

    #[test]
    fn extension_origin_requires_a_real_chrome_extension_id() {
        assert_eq!(
            chrome_extension_origin("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"
        );
        assert!(chrome_extension_origin("short").is_err());
        assert!(chrome_extension_origin("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
        assert!(chrome_extension_origin("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_err());
    }

    #[test]
    fn configured_development_identity_is_valid_and_release_fails_closed() {
        assert_eq!(
            configured_extension_id(ExtensionChannel::Development).unwrap(),
            "agokdhalkipifklmbipkgmfakdcaekbj"
        );
        assert!(configured_extension_id(ExtensionChannel::Release).is_err());
        assert_eq!(
            configured_firefox_extension_id(ExtensionChannel::Development).unwrap(),
            "development@mindcanary.local"
        );
        assert!(configured_firefox_extension_id(ExtensionChannel::Release).is_err());
    }

    #[test]
    fn manifest_uses_chrome_native_messaging_shape() {
        let manifest = manifest_for_host(
            NativeMessagingBrowser::Chrome,
            Path::new("/opt/mindcanary/bin/mindcanary-native-host"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        let value = serde_json::to_value(manifest).unwrap();

        assert_eq!(value["name"], NATIVE_HOST_NAME);
        assert_eq!(value["type"], "stdio");
        assert_eq!(value["path"], "/opt/mindcanary/bin/mindcanary-native-host");
        assert_eq!(
            value["allowed_origins"],
            json!(["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"])
        );
    }

    #[test]
    fn manifest_rejects_relative_host_paths() {
        assert!(
            manifest_for_host(
                NativeMessagingBrowser::Chrome,
                Path::new("target/debug/mindcanary-native-host"),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
            .is_err()
        );
    }

    #[test]
    fn linux_manifest_dir_prefers_xdg_config_home() {
        let dir = user_manifest_dir_from_values(
            NativeMessagingBrowser::Chrome,
            Some(PathBuf::from("/tmp/mc-config")),
            Some(PathBuf::from("/home/tester")),
        )
        .unwrap();

        assert_eq!(
            dir,
            PathBuf::from("/tmp/mc-config/google-chrome/NativeMessagingHosts")
        );
    }

    #[test]
    fn linux_manifest_dir_falls_back_to_home_config() {
        let dir = user_manifest_dir_from_values(
            NativeMessagingBrowser::Chromium,
            None,
            Some(PathBuf::from("/home/tester")),
        )
        .unwrap();

        assert_eq!(
            dir,
            PathBuf::from("/home/tester/.config/chromium/NativeMessagingHosts")
        );
    }

    #[test]
    fn install_writes_manifest_into_requested_directory() {
        let directory = tempfile::TempDir::new().unwrap();
        let manifest_path = install_native_host_manifest(
            NativeMessagingBrowser::Chrome,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Path::new("/opt/mindcanary/bin/mindcanary-native-host"),
            Some(directory.path()),
        )
        .unwrap();

        assert_eq!(
            manifest_path,
            directory.path().join(format!("{NATIVE_HOST_NAME}.json"))
        );
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
        assert_eq!(
            value["allowed_origins"],
            json!(["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"])
        );
    }

    #[test]
    fn firefox_manifest_uses_add_on_id_and_mozilla_user_directory() {
        let manifest = manifest_for_host(
            NativeMessagingBrowser::Firefox,
            Path::new("/opt/mindcanary/bin/mindcanary-native-host"),
            "development@mindcanary.local",
        )
        .unwrap();
        let value = serde_json::to_value(manifest).unwrap();
        assert_eq!(
            value["allowed_extensions"],
            json!(["development@mindcanary.local"])
        );
        assert!(value.get("allowed_origins").is_none());

        let dir = user_manifest_dir_from_values(
            NativeMessagingBrowser::Firefox,
            Some(PathBuf::from("/tmp/ignored-xdg")),
            Some(PathBuf::from("/home/tester")),
        )
        .unwrap();
        assert_eq!(
            dir,
            PathBuf::from("/home/tester/.mozilla/native-messaging-hosts")
        );
    }

    #[test]
    fn manifest_inspection_distinguishes_missing_ready_and_repair_states() {
        let directory = tempfile::TempDir::new().unwrap();
        let host_path = Path::new("/opt/mindcanary/bin/mindcanary-native-host");

        let missing = inspect_native_host_manifest(
            NativeMessagingBrowser::Chrome,
            ExtensionChannel::Development,
            host_path,
            Some(directory.path()),
        )
        .unwrap();
        assert_eq!(missing.health, NativeHostManifestHealth::Missing);

        install_native_host_manifest(
            NativeMessagingBrowser::Chrome,
            &configured_extension_id(ExtensionChannel::Development).unwrap(),
            host_path,
            Some(directory.path()),
        )
        .unwrap();
        let ready = inspect_native_host_manifest(
            NativeMessagingBrowser::Chrome,
            ExtensionChannel::Development,
            host_path,
            Some(directory.path()),
        )
        .unwrap();
        assert_eq!(ready.health, NativeHostManifestHealth::Ready);

        fs::write(&ready.path, b"{\"name\":\"wrong\"}").unwrap();
        let malformed = inspect_native_host_manifest(
            NativeMessagingBrowser::Chrome,
            ExtensionChannel::Development,
            host_path,
            Some(directory.path()),
        )
        .unwrap();
        assert_eq!(malformed.health, NativeHostManifestHealth::NeedsRepair);
    }

    #[test]
    fn manifest_inspection_detects_wrong_host_path_or_origin() {
        let directory = tempfile::TempDir::new().unwrap();
        install_native_host_manifest(
            NativeMessagingBrowser::Chrome,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Path::new("/tmp/wrong-host"),
            Some(directory.path()),
        )
        .unwrap();

        let status = inspect_native_host_manifest(
            NativeMessagingBrowser::Chrome,
            ExtensionChannel::Development,
            Path::new("/opt/mindcanary/bin/mindcanary-native-host"),
            Some(directory.path()),
        )
        .unwrap();

        assert_eq!(status.health, NativeHostManifestHealth::NeedsRepair);
    }

    #[test]
    fn uninstall_removes_manifest_from_requested_directory() {
        let directory = tempfile::TempDir::new().unwrap();
        let manifest_path = install_native_host_manifest(
            NativeMessagingBrowser::Chrome,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Path::new("/opt/mindcanary/bin/mindcanary-native-host"),
            Some(directory.path()),
        )
        .unwrap();
        assert!(manifest_path.exists());

        let removed_path =
            uninstall_native_host_manifest(NativeMessagingBrowser::Chrome, Some(directory.path()))
                .unwrap();

        assert_eq!(removed_path, manifest_path);
        assert!(!removed_path.exists());
    }

    #[test]
    fn uninstall_is_idempotent_for_missing_manifest() {
        let directory = tempfile::TempDir::new().unwrap();

        let removed_path = uninstall_native_host_manifest(
            NativeMessagingBrowser::Chromium,
            Some(directory.path()),
        )
        .unwrap();

        assert_eq!(
            removed_path,
            directory.path().join(format!("{NATIVE_HOST_NAME}.json"))
        );
        assert!(!removed_path.exists());
    }

    #[tokio::test]
    async fn forwards_a_health_request_to_the_daemon() {
        let runtime_dir =
            std::env::temp_dir().join(format!("mindcanary-host-test-{}", uuid::Uuid::now_v7()));
        let socket_path = runtime_dir.join("mindcanaryd.sock");
        let server_path = socket_path.clone();
        let data_dir = tempfile::TempDir::new().unwrap();
        let key = mindcanary_storage::DatabaseKey::from_bytes([17; 32]);
        let store =
            mindcanary_storage::EncryptedStore::open(data_dir.path().join("mindcanary.db"), &key)
                .unwrap();
        let state = std::sync::Arc::new(mindcanaryd::DaemonState::new(store));
        let server =
            tokio::spawn(async move { mindcanaryd::run_with_state(&server_path, state).await });

        for _ in 0..100 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(socket_path.exists());

        let request = ProtocolRequest::Health {
            protocol_version: PROTOCOL_VERSION,
        };
        let response = forward_request(&socket_path, &request).await.unwrap();
        let response: ProtocolResponse = serde_json::from_slice(&response).unwrap();

        assert!(matches!(
            response,
            ProtocolResponse::Health {
                status: mindcanary_protocol::ServiceStatus::Ready,
                ..
            }
        ));

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_dir_all(runtime_dir);
    }
}
