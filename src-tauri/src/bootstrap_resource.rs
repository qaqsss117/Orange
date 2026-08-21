use std::{fmt, path::Path, sync::Arc};

#[cfg(orange_embedded_bootstrap)]
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::IpAddr,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(orange_embedded_bootstrap)]
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

#[cfg(orange_embedded_bootstrap)]
use crate::bootstrap_http::PinnedHttpsClient;
use crate::control_plane::ManagedControlPlane;
#[cfg(orange_embedded_bootstrap)]
use keyring::{Entry, Error as KeyringError};
#[cfg(orange_embedded_bootstrap)]
use orange_bootstrap::{
    BootstrapCache, BootstrapDiscovery, BootstrapKey, BootstrapLocatorConfig, BootstrapManifest,
    BootstrapSelectionError, BootstrapSelectionSource, CacheKey, CachedBootstrapState,
    EmbeddedBootstrapArtifact, FetchedBootstrapArtifact, TxtLocatorDocument, VerifyingKey,
    activate_with_fallback, decrypt, open_cache_state, seal_cache_state,
    validate_verifying_key_set,
};
#[cfg(orange_embedded_bootstrap)]
use orange_control_plane_host::{ControlPlaneRequest, HostOptions};

#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_ENVELOPE: &[u8] = include_bytes!(env!("ORANGE_BOOTSTRAP_ENVELOPE_PATH"));
#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_MANIFEST: &str = include_str!(env!("ORANGE_BOOTSTRAP_MANIFEST_PATH"));
#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_KEY: &[u8; 32] = include_bytes!(env!("ORANGE_BOOTSTRAP_KEY_PATH"));

#[allow(dead_code)]
pub(crate) fn start_embedded(
    control_plane: &Arc<ManagedControlPlane>,
) -> Result<bool, EmbeddedBootstrapError> {
    #[cfg(not(orange_embedded_bootstrap))]
    {
        let _ = control_plane;
        Ok(false)
    }

    #[cfg(orange_embedded_bootstrap)]
    {
        let manifest: BootstrapManifest = serde_json::from_str(EMBEDDED_MANIFEST)
            .map_err(|_| EmbeddedBootstrapError::InvalidResource)?;
        let key = BootstrapKey::from_bytes(*EMBEDDED_KEY);
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| EmbeddedBootstrapError::Clock)?
            .as_secs();
        start_control_plane_with_candidates(
            control_plane,
            EMBEDDED_ENVELOPE,
            &manifest,
            &key,
            now_unix,
        )
        .map_err(|_| EmbeddedBootstrapError::Unavailable)?;
        Ok(true)
    }
}

#[cfg(orange_embedded_bootstrap)]
pub(crate) fn start_with_fallback(
    control_plane: &Arc<ManagedControlPlane>,
    app_data_dir: &Path,
) -> Result<bool, EmbeddedBootstrapError> {
    let embedded_manifest: BootstrapManifest = serde_json::from_str(EMBEDDED_MANIFEST)
        .map_err(|_| EmbeddedBootstrapError::InvalidResource)?;
    let embedded = EmbeddedBootstrapArtifact {
        manifest: embedded_manifest,
        envelope: EMBEDDED_ENVELOPE.to_vec(),
    };
    let key = orange_bootstrap::BootstrapKey::from_bytes(*EMBEDDED_KEY);
    let locator = locator_config()?;
    let verifying_keys = if locator.manifest_urls.is_empty() {
        Vec::new()
    } else {
        verifying_keys()?
    };
    let cache = DesktopBootstrapCache::new(app_data_dir)?;
    let discovery = HttpBootstrapDiscovery::new()?;
    let now_unix = current_unix_time()?;
    let result = activate_with_fallback(
        &locator,
        &discovery,
        &cache,
        &embedded,
        &verifying_keys,
        embedded.manifest.channel.as_str(),
        env!("CARGO_PKG_VERSION"),
        now_unix,
        |_, manifest, envelope| {
            start_control_plane_with_candidates(control_plane, envelope, manifest, &key, now_unix)
                .map_err(|error| match error {
                    EmbeddedBootstrapError::InvalidResource | EmbeddedBootstrapError::Clock => {
                        orange_bootstrap::BootstrapActivationError::InvalidResource
                    }
                    EmbeddedBootstrapError::Unavailable | EmbeddedBootstrapError::Cache => {
                        orange_bootstrap::BootstrapActivationError::Unavailable
                    }
                })
        },
    );
    result
        .map(|_| true)
        .map_err(|_| EmbeddedBootstrapError::Unavailable)
}

#[cfg(not(orange_embedded_bootstrap))]
pub(crate) fn start_with_fallback(
    control_plane: &Arc<ManagedControlPlane>,
    app_data_dir: &Path,
) -> Result<bool, EmbeddedBootstrapError> {
    let _ = (control_plane, app_data_dir);
    Ok(false)
}

#[cfg(orange_embedded_bootstrap)]
fn current_unix_time() -> Result<u64, EmbeddedBootstrapError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| EmbeddedBootstrapError::Clock)
}

#[cfg(orange_embedded_bootstrap)]
fn start_control_plane_with_candidates(
    control_plane: &Arc<ManagedControlPlane>,
    envelope: &[u8],
    manifest: &BootstrapManifest,
    key: &BootstrapKey,
    now_unix: u64,
) -> Result<(), EmbeddedBootstrapError> {
    let mut secret = decrypt(envelope, manifest, key, now_unix)
        .map_err(|_| EmbeddedBootstrapError::InvalidResource)?;
    control_plane
        .start(&mut secret, 0, HostOptions::default())
        .map_err(|_| EmbeddedBootstrapError::Unavailable)?;

    // Promotion to last-known-good requires a real request through the
    // bootstrap outbound. The sidecar applies the candidate/API combination
    // limit, circuit breaker, and GET retry policy to this probe.
    let healthy = control_plane
        .execute(ControlPlaneRequest::get_primary(
            "/api/v1/guest/comm/config",
        ))
        .is_ok_and(|response| (200..=299).contains(&response.status_code()));
    if healthy {
        return Ok(());
    }
    let _ = control_plane.stop();
    Err(EmbeddedBootstrapError::Unavailable)
}

#[cfg(orange_embedded_bootstrap)]
fn locator_config() -> Result<BootstrapLocatorConfig, EmbeddedBootstrapError> {
    let manifest_urls: Vec<String> = env!("ORANGE_BOOTSTRAP_MANIFEST_URLS")
        .split(';')
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .collect();
    let txt_record_names: Vec<String> = env!("ORANGE_BOOTSTRAP_TXT_NAMES")
        .split(';')
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .collect();
    let dns_resolvers: Vec<IpAddr> = (!manifest_urls.is_empty() || !txt_record_names.is_empty())
        .then_some(["1.1.1.1", "8.8.8.8"])
        .into_iter()
        .flatten()
        .into_iter()
        .map(|value| {
            value
                .parse::<IpAddr>()
                .expect("hardcoded resolver is valid")
        })
        .collect();
    let remote_disabled = manifest_urls.is_empty() && txt_record_names.is_empty();
    let locator = BootstrapLocatorConfig {
        manifest_urls,
        txt_record_names,
        dns_resolvers,
        refresh_budget_ms: 4_000,
    };
    // An unsigned development build can omit remote discovery completely.
    // The selection layer will then use the embedded artifact directly.
    if remote_disabled {
        return Ok(locator);
    }
    locator
        .validate()
        .map_err(|_| EmbeddedBootstrapError::InvalidResource)?;
    Ok(locator)
}

#[cfg(orange_embedded_bootstrap)]
fn verifying_keys() -> Result<Vec<VerifyingKey>, EmbeddedBootstrapError> {
    let keys = env!("ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS")
        .split(';')
        .filter(|value| !value.trim().is_empty())
        .map(|entry| {
            let (key_id, key) = entry
                .split_once('=')
                .ok_or(EmbeddedBootstrapError::InvalidResource)?;
            VerifyingKey::from_base64(key_id.to_owned(), key)
                .map_err(|_| EmbeddedBootstrapError::InvalidResource)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_verifying_key_set(&keys).map_err(|_| EmbeddedBootstrapError::InvalidResource)?;
    Ok(keys)
}

#[cfg(orange_embedded_bootstrap)]
struct HttpBootstrapDiscovery {
    client: PinnedHttpsClient,
}

#[cfg(orange_embedded_bootstrap)]
impl HttpBootstrapDiscovery {
    fn new() -> Result<Self, EmbeddedBootstrapError> {
        PinnedHttpsClient::new(
            &[
                "1.1.1.1".parse().expect("resolver"),
                "8.8.8.8".parse().expect("resolver"),
            ],
            format!("Orange/{}/bootstrap", env!("CARGO_PKG_VERSION")),
        )
        .map(|client| Self { client })
        .ok_or(EmbeddedBootstrapError::Unavailable)
    }

    fn fetch_artifact(
        &self,
        manifest_url: &str,
        deadline: Instant,
    ) -> Option<FetchedBootstrapArtifact> {
        let manifest_bytes = self.client.get_bounded(manifest_url, deadline, 32 * 1024)?;
        let manifest =
            serde_json::from_slice::<orange_bootstrap::RemoteBootstrapManifest>(&manifest_bytes)
                .ok()?;
        let envelope = self
            .client
            .get_bounded(&manifest.envelope_url, deadline, 128 * 1024)?;
        Some(FetchedBootstrapArtifact { manifest, envelope })
    }
}

#[cfg(orange_embedded_bootstrap)]
impl BootstrapDiscovery for HttpBootstrapDiscovery {
    fn fetch_artifacts(
        &self,
        manifest_urls: &[String],
        deadline: Instant,
    ) -> Vec<FetchedBootstrapArtifact> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            for manifest_url in manifest_urls {
                let sender = sender.clone();
                scope.spawn(move || {
                    let _ = sender.send(self.fetch_artifact(manifest_url, deadline));
                });
            }
        });
        drop(sender);
        receiver.into_iter().flatten().collect()
    }

    fn discover_txt(
        &self,
        names: &[String],
        resolvers: &[IpAddr],
        deadline: Instant,
    ) -> Vec<TxtLocatorDocument> {
        self.client
            .txt_records(names, deadline)
            .into_iter()
            .filter_map(|data| {
                let record = join_txt_fragments(&data)?;
                let encoded = record.strip_prefix("orange-bootstrap-v1:")?;
                let payload = URL_SAFE_NO_PAD.decode(encoded).ok()?;
                serde_json::from_slice(&payload).ok()
            })
            .collect()
    }
}

#[cfg(any(orange_embedded_bootstrap, test))]
fn join_txt_fragments(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.starts_with('"') {
        return Some(value.to_owned());
    }
    let mut joined = String::new();
    let mut characters = value.chars().peekable();
    while characters.peek().is_some() {
        while characters
            .next_if(|character| character.is_whitespace())
            .is_some()
        {}
        if characters.next()? != '"' {
            return None;
        }
        loop {
            match characters.next()? {
                '"' => break,
                '\\' => joined.push(characters.next()?),
                character if !character.is_control() => joined.push(character),
                _ => return None,
            }
        }
    }
    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::join_txt_fragments;

    #[test]
    fn txt_fragments_are_joined_without_dns_quotes() {
        assert_eq!(
            join_txt_fragments("\"orange-bootstrap-v1:abc\" \"def\"").as_deref(),
            Some("orange-bootstrap-v1:abcdef")
        );
        assert_eq!(
            join_txt_fragments("not-quoted").as_deref(),
            Some("not-quoted")
        );
        assert_eq!(join_txt_fragments("\"unterminated"), None);
        assert_eq!(join_txt_fragments("\"valid\" trailing"), None);
        assert_eq!(join_txt_fragments("\"invalid\nrecord\""), None);
    }
}

#[cfg(orange_embedded_bootstrap)]
struct DesktopBootstrapCache {
    path: PathBuf,
    previous_path: PathBuf,
    key_entry: Entry,
}

#[cfg(orange_embedded_bootstrap)]
impl DesktopBootstrapCache {
    fn new(app_data_dir: &Path) -> Result<Self, EmbeddedBootstrapError> {
        fs::create_dir_all(app_data_dir).map_err(|_| EmbeddedBootstrapError::Cache)?;
        let key_entry = Entry::new("com.orange.vpn", "orange.bootstrap-cache-key")
            .map_err(|_| EmbeddedBootstrapError::Cache)?;
        Ok(Self {
            path: app_data_dir.join("bootstrap.cache"),
            previous_path: app_data_dir.join("bootstrap.cache.previous"),
            key_entry,
        })
    }

    fn cache_key(&self) -> Result<CacheKey, BootstrapSelectionError> {
        match self.key_entry.get_secret() {
            Ok(value) if value.len() == 32 => {
                let mut key = [0_u8; 32];
                key.copy_from_slice(&value);
                Ok(CacheKey::from_bytes(key))
            }
            Ok(_) => Err(BootstrapSelectionError::Cache),
            Err(KeyringError::NoEntry) => {
                let mut key = [0_u8; 32];
                getrandom::fill(&mut key).map_err(|_| BootstrapSelectionError::Cache)?;
                self.key_entry
                    .set_secret(&key)
                    .map_err(|_| BootstrapSelectionError::Cache)?;
                Ok(CacheKey::from_bytes(key))
            }
            Err(_) => Err(BootstrapSelectionError::Cache),
        }
    }
}

#[cfg(orange_embedded_bootstrap)]
impl BootstrapCache for DesktopBootstrapCache {
    fn load(&self) -> Result<Option<CachedBootstrapState>, BootstrapSelectionError> {
        let key = self.cache_key()?;
        for path in [&self.path, &self.previous_path] {
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => continue,
            };
            if let Ok(state) = open_cache_state(&bytes, &key) {
                return Ok(Some(state));
            }
        }
        Ok(None)
    }

    fn store(&self, state: &CachedBootstrapState) -> Result<(), BootstrapSelectionError> {
        let bytes = seal_cache_state(state, &self.cache_key()?)?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| BootstrapSelectionError::Cache)?;
        file.write_all(&bytes)
            .map_err(|_| BootstrapSelectionError::Cache)?;
        file.sync_all()
            .map_err(|_| BootstrapSelectionError::Cache)?;
        drop(file);
        // Keep one recoverable slot.  Rename is atomic on the same volume;
        // the previous slot is only removed after the new temporary file is
        // durable.
        if self.previous_path.exists() {
            fs::remove_file(&self.previous_path).map_err(|_| BootstrapSelectionError::Cache)?;
        }
        if self.path.exists() {
            fs::rename(&self.path, &self.previous_path)
                .map_err(|_| BootstrapSelectionError::Cache)?;
        }
        fs::rename(temporary, &self.path).map_err(|_| BootstrapSelectionError::Cache)
    }
}

#[cfg_attr(not(orange_embedded_bootstrap), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmbeddedBootstrapError {
    InvalidResource,
    Clock,
    Unavailable,
    Cache,
}

impl fmt::Display for EmbeddedBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidResource => "embedded-bootstrap-invalid",
            Self::Clock => "embedded-bootstrap-clock-unavailable",
            Self::Unavailable => "embedded-bootstrap-unavailable",
            Self::Cache => "embedded-bootstrap-cache-unavailable",
        })
    }
}

impl std::error::Error for EmbeddedBootstrapError {}
