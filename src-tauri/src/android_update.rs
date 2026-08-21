use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use orange_bootstrap::{
    AndroidUpdateManifest, TxtLocatorDocument, VerifyingKey, validate_verifying_key_set,
    verify_android_update_manifest, verify_txt_locator,
};
use orange_domain::{AndroidUpdateRequest, AndroidUpdateResponse, CommandError, ErrorCode};
use serde::{Deserialize, Serialize};
use tauri::{
    Manager, Runtime,
    plugin::{Builder, PluginHandle, TauriPlugin, mobile::PluginInvokeError},
};

use crate::bootstrap_http::PinnedHttpsClient;

const PLUGIN_IDENTIFIER: &str = "com.orange.vpn.platform";
const PLUGIN_CLASS: &str = "AndroidUpdateInstallerPlugin";
const CHECK_BUDGET: Duration = Duration::from_secs(4);
const DOWNLOAD_BUDGET: Duration = Duration::from_secs(5 * 60);

pub(crate) struct AndroidUpdater<R: Runtime> {
    handle: PluginHandle<R>,
    selected: Arc<Mutex<Option<AndroidUpdateManifest>>>,
}

impl<R: Runtime> Clone for AndroidUpdater<R> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            selected: Arc::clone(&self.selected),
        }
    }
}

impl<R: Runtime> AndroidUpdater<R> {
    fn new(handle: PluginHandle<R>) -> Self {
        Self {
            handle,
            selected: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn check(&self) -> Result<AndroidUpdateResponse, CommandError> {
        let keys = verifying_keys()?;
        let deadline = Instant::now() + CHECK_BUDGET;
        let client = http_client()?;
        let mut urls = split_env(env!("ORANGE_ANDROID_UPDATE_MANIFEST_URLS"));
        let mut manifests = fetch_manifests(&client, &urls, deadline);
        if manifests.is_empty() && Instant::now() < deadline {
            urls = discover_manifest_urls(&keys, deadline);
            manifests = fetch_manifests(&client, &urls, deadline);
        }
        let now = current_unix()?;
        let current_version = env!("ORANGE_ANDROID_VERSION_CODE")
            .parse::<u64>()
            .map_err(|_| service_error())?;
        let mut selected = manifests
            .into_iter()
            .filter(|manifest| {
                verify_android_update_manifest(
                    manifest,
                    &keys,
                    env!("ORANGE_ANDROID_PACKAGE_ID"),
                    current_version,
                    env!("ORANGE_ANDROID_SIGNING_CERT_SHA256"),
                    now,
                )
                .is_ok()
            })
            .max_by_key(|manifest| manifest.version_code);
        let response = match selected.as_ref() {
            Some(manifest) => {
                AndroidUpdateResponse::new(true, Some(manifest.version_name.clone()), false, false)
            }
            None => AndroidUpdateResponse::new(false, None, false, false),
        };
        *lock(&self.selected) = selected.take();
        Ok(response)
    }

    pub(crate) fn install(&self) -> Result<AndroidUpdateResponse, CommandError> {
        let manifest = lock(&self.selected).clone().ok_or_else(service_error)?;
        let first = manifest.apk_mirrors.first().ok_or_else(service_error)?;
        let prepared: PrepareResponse = self
            .handle
            .run_mobile_plugin(
                "prepare",
                PrepareRequest {
                    protocol_version: 1,
                },
            )
            .map_err(map_invoke_error)?;
        if prepared.permission_required {
            return Ok(AndroidUpdateResponse::new(
                true,
                Some(manifest.version_name),
                true,
                false,
            ));
        }
        let client = http_client()?;
        let deadline = Instant::now() + DOWNLOAD_BUDGET;
        let destination = std::path::Path::new(&prepared.apk_path);
        let downloaded = manifest.apk_mirrors.iter().any(|mirror| {
            client.download_verified(
                &mirror.url,
                deadline,
                destination,
                mirror.bytes,
                &mirror.sha256,
            )
        });
        if !downloaded {
            let _: Result<CleanupResponse, _> = self.handle.run_mobile_plugin(
                "cleanup",
                CleanupRequest {
                    protocol_version: 1,
                },
            );
            return Err(service_error());
        }
        let response: InstallResponse = self
            .handle
            .run_mobile_plugin(
                "install",
                InstallRequest {
                    protocol_version: 1,
                    apk_path: prepared.apk_path,
                    sha256: first.sha256.clone(),
                    expected_bytes: first.bytes,
                    package_name: manifest.package_name.clone(),
                    version_code: manifest.version_code,
                    certificate_sha256: manifest.signing_certificate_sha256.clone(),
                },
            )
            .map_err(map_invoke_error)?;
        Ok(AndroidUpdateResponse::new(
            true,
            Some(manifest.version_name),
            response.permission_required,
            response.started,
        ))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareRequest {
    protocol_version: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PrepareResponse {
    permission_required: bool,
    apk_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallRequest {
    protocol_version: u16,
    apk_path: String,
    sha256: String,
    expected_bytes: u64,
    package_name: String,
    version_code: u64,
    certificate_sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallResponse {
    permission_required: bool,
    started: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupRequest {
    protocol_version: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CleanupResponse {}

fn fetch_manifests(
    client: &PinnedHttpsClient,
    urls: &[String],
    deadline: Instant,
) -> Vec<AndroidUpdateManifest> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        for url in urls {
            let sender = sender.clone();
            scope.spawn(move || {
                let value = client
                    .get_bounded(url, deadline, 64 * 1024)
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok());
                let _ = sender.send(value);
            });
        }
    });
    drop(sender);
    receiver.into_iter().flatten().collect()
}

fn discover_manifest_urls(keys: &[VerifyingKey], deadline: Instant) -> Vec<String> {
    let names = split_env(env!("ORANGE_ANDROID_UPDATE_TXT_NAMES"));
    if names.is_empty() {
        return Vec::new();
    }
    let Some(client) = PinnedHttpsClient::new(
        &[
            "1.1.1.1".parse().expect("resolver"),
            "8.8.8.8".parse().expect("resolver"),
        ],
        format!("Orange/{}/android-update", env!("CARGO_PKG_VERSION")),
    ) else {
        return Vec::new();
    };
    let now = match current_unix() {
        Ok(now) => now,
        Err(_) => return Vec::new(),
    };
    for data in client.txt_records(&names, deadline) {
        let Some(record) = join_txt_fragments(&data) else {
            continue;
        };
        let Some(encoded) = record.strip_prefix("orange-bootstrap-v1:") else {
            continue;
        };
        let Ok(payload) = URL_SAFE_NO_PAD.decode(encoded) else {
            continue;
        };
        let Ok(locator) = serde_json::from_slice::<TxtLocatorDocument>(&payload) else {
            continue;
        };
        if verify_txt_locator(&locator, keys, now, 0).is_ok() {
            return locator.manifest_urls;
        }
    }
    Vec::new()
}

fn join_txt_fragments(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.starts_with('"') {
        return Some(value.to_owned());
    }
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while chars.peek().is_some() {
        while chars
            .next_if(|character| character.is_whitespace())
            .is_some()
        {}
        if chars.next()? != '"' {
            return None;
        }
        loop {
            match chars.next()? {
                '"' => break,
                '\\' => output.push(chars.next()?),
                character if !character.is_control() => output.push(character),
                _ => return None,
            }
        }
    }
    (!output.is_empty()).then_some(output)
}

fn verifying_keys() -> Result<Vec<VerifyingKey>, CommandError> {
    let keys = split_env(env!("ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS"))
        .into_iter()
        .map(|entry| {
            let (id, value) = entry.split_once('=').ok_or_else(service_error)?;
            VerifyingKey::from_base64(id.to_owned(), value).map_err(|_| service_error())
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_verifying_key_set(&keys).map_err(|_| service_error())?;
    Ok(keys)
}

fn split_env(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn http_client() -> Result<PinnedHttpsClient, CommandError> {
    PinnedHttpsClient::new(
        &[
            "1.1.1.1".parse().expect("resolver"),
            "8.8.8.8".parse().expect("resolver"),
        ],
        format!("Orange/{}/android-update", env!("CARGO_PKG_VERSION")),
    )
    .ok_or_else(service_error)
}

fn current_unix() -> Result<u64, CommandError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| service_error())
}

fn map_invoke_error(_: PluginInvokeError) -> CommandError {
    service_error()
}
fn service_error() -> CommandError {
    CommandError::from_code(ErrorCode::Service)
}
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[tauri::command]
pub(crate) fn check_android_update(
    request: AndroidUpdateRequest,
    updater: tauri::State<'_, AndroidUpdater<tauri::Wry>>,
) -> Result<AndroidUpdateResponse, CommandError> {
    request.validate()?;
    updater.check()
}

#[tauri::command]
pub(crate) fn install_android_update(
    request: AndroidUpdateRequest,
    updater: tauri::State<'_, AndroidUpdater<tauri::Wry>>,
) -> Result<AndroidUpdateResponse, CommandError> {
    request.validate()?;
    updater.install()
}

pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("orange-android-update")
        .setup(|app, api| {
            let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, PLUGIN_CLASS)?;
            app.manage(AndroidUpdater::new(handle));
            Ok(())
        })
        .build()
}
