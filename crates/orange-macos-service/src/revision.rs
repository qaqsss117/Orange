use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use orange_domain::DataPlaneState;
use orange_platform::{
    CancellationToken, ConfigurationRevision, DataPlaneCandidateHealth, DataPlaneNodeBackend,
    PlatformVpnAdapter, PlatformVpnError, SelectorCatalog, SupervisedVpnAdapter,
};
use orange_service_core::{
    MAX_REVISION_CHUNK_BYTES, ServiceSubscriptionBackend, inspect_runtime_config,
    normalize_runtime_config, prepare_probe_config,
};
use sha2::{Digest, Sha256};

use crate::sidecar::{CandidateProcess, MacosDataPlaneBackend};

const ACTIVE_FILE: &str = "active-revision.v1";
const ACTIVE_TEMP: &str = ".active-revision.v1.installing";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(8);
const START_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone)]
pub struct MacosRevisionBackend {
    inner: Arc<Inner>,
}

struct Inner {
    data_plane: MacosDataPlaneBackend,
    adapter: SupervisedVpnAdapter<MacosDataPlaneBackend>,
    install: Mutex<Option<InstallState>>,
    candidate: Mutex<Option<CandidateState>>,
    active: Mutex<Option<ConfigurationRevision>>,
}

struct InstallState {
    revision: ConfigurationRevision,
    expected_bytes: usize,
    expected_sha256: String,
    selector_id: String,
    node_id: String,
    written: usize,
    digest: Sha256,
    path: PathBuf,
    file: Option<File>,
}

struct CandidateState {
    revision: ConfigurationRevision,
    selector_id: String,
    node_id: String,
    dns_independent: bool,
    path: PathBuf,
    process: CandidateProcess,
    health: Option<DataPlaneCandidateHealth>,
    previous: Option<ConfigurationRevision>,
}

impl MacosRevisionBackend {
    pub fn installed(
        data_plane: MacosDataPlaneBackend,
        adapter: SupervisedVpnAdapter<MacosDataPlaneBackend>,
    ) -> Result<Self, PlatformVpnError> {
        let root = data_plane.revision_root();
        fs::create_dir_all(root).map_err(|_| PlatformVpnError::Unavailable)?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))
            .map_err(|_| PlatformVpnError::Unavailable)?;
        let active = load_active(root)?;
        Ok(Self {
            inner: Arc::new(Inner {
                data_plane,
                adapter,
                install: Mutex::new(None),
                candidate: Mutex::new(None),
                active: Mutex::new(active),
            }),
        })
    }

    pub fn recover_on_start(&self) -> Result<(), PlatformVpnError> {
        if let Some(revision) = *lock(&self.inner.active) {
            restore_runtime(&self.inner.adapter, Some(revision))?;
        }
        Ok(())
    }

    fn revision_path(&self, revision: ConfigurationRevision) -> PathBuf {
        self.inner
            .data_plane
            .revision_root()
            .join(format!("{}.json", revision.get()))
    }

    fn temporary_path(&self, revision: ConfigurationRevision) -> PathBuf {
        self.inner
            .data_plane
            .revision_root()
            .join(format!(".{}.installing", revision.get()))
    }

    fn probe_path(&self, revision: ConfigurationRevision) -> PathBuf {
        self.inner
            .data_plane
            .revision_root()
            .join(format!(".{}.probe.json", revision.get()))
    }

    fn persist_active(
        &self,
        revision: Option<ConfigurationRevision>,
    ) -> Result<(), PlatformVpnError> {
        persist_active(self.inner.data_plane.revision_root(), revision)
    }
}

impl ServiceSubscriptionBackend for MacosRevisionBackend {
    fn begin_revision_install(
        &self,
        revision: ConfigurationRevision,
        total_bytes: usize,
        sha256: &str,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), PlatformVpnError> {
        if total_bytes == 0
            || total_bytes > 1 << 20
            || !valid_sha256(sha256)
            || !valid_id(selector_id)
            || !valid_id(node_id)
        {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let mut install = lock(&self.inner.install);
        if install.is_some() {
            return Err(PlatformVpnError::OperationInProgress);
        }
        let path = self.temporary_path(revision);
        remove_regular(&path)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        *install = Some(InstallState {
            revision,
            expected_bytes: total_bytes,
            expected_sha256: sha256.to_owned(),
            selector_id: selector_id.to_owned(),
            node_id: node_id.to_owned(),
            written: 0,
            digest: Sha256::new(),
            path,
            file: Some(file),
        });
        Ok(())
    }

    fn install_revision_chunk(
        &self,
        revision: ConfigurationRevision,
        offset: usize,
        payload: &[u8],
    ) -> Result<(), PlatformVpnError> {
        if payload.is_empty() || payload.len() > MAX_REVISION_CHUNK_BYTES {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let mut install = lock(&self.inner.install);
        let state = install
            .as_mut()
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        let end = offset
            .checked_add(payload.len())
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        if state.revision != revision || state.written != offset || end > state.expected_bytes {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        state
            .file
            .as_mut()
            .ok_or(PlatformVpnError::ProtocolViolation)?
            .write_all(payload)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        state.digest.update(payload);
        state.written = end;
        Ok(())
    }

    fn commit_revision_install(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<(), PlatformVpnError> {
        let mut state = lock(&self.inner.install)
            .take()
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        if state.revision != revision || state.written != state.expected_bytes {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        let file = state
            .file
            .take()
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        file.sync_all().map_err(|_| PlatformVpnError::Unavailable)?;
        drop(file);
        if format!("{:x}", std::mem::take(&mut state.digest).finalize()) != state.expected_sha256 {
            remove_regular(&state.path)?;
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let raw = fs::read(&state.path).map_err(|_| PlatformVpnError::Unavailable)?;
        let normalized = normalize_runtime_config(&raw, Some(self.inner.data_plane.rules_root()))?;
        let first = normalized
            .catalog()
            .groups()
            .first()
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        if first.id() != state.selector_id || first.default_node_id() != state.node_id {
            remove_regular(&state.path)?;
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&state.path)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        file.write_all(normalized.json())
            .map_err(|_| PlatformVpnError::Unavailable)?;
        file.sync_all().map_err(|_| PlatformVpnError::Unavailable)?;
        drop(file);
        let destination = self.revision_path(revision);
        if destination.exists() {
            let existing = read_regular(&destination, 1 << 20)?;
            if existing != normalized.json() {
                remove_regular(&state.path)?;
                return Err(PlatformVpnError::PermissionDenied);
            }
            remove_regular(&state.path)?;
            return Ok(());
        }
        fs::rename(&state.path, &destination).map_err(|_| PlatformVpnError::Unavailable)?;
        sync_directory(self.inner.data_plane.revision_root())
    }

    fn start_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let mut candidate = lock(&self.inner.candidate);
        if candidate.is_some() {
            return Err(PlatformVpnError::OperationInProgress);
        }
        let previous = *lock(&self.inner.active);
        let snapshot = self.inner.adapter.snapshot()?;
        if snapshot.has_active_instance() {
            self.inner.adapter.stop(snapshot.instance_id())?;
        }
        let bytes = read_regular(&self.revision_path(revision), 1 << 20)?;
        let probe = prepare_probe_config(&bytes, Some(self.inner.data_plane.rules_root()))?;
        let path = self.probe_path(revision);
        write_new(&path, probe.json())?;
        let process = match self.inner.data_plane.start_probe(revision, &path) {
            Ok(process) => process,
            Err(error) => {
                let _ = remove_regular(&path);
                let _ = restore_runtime(&self.inner.adapter, previous);
                return Err(error);
            }
        };
        *candidate = Some(CandidateState {
            revision,
            selector_id: probe.selector_id().to_owned(),
            node_id: probe.default_node_id().to_owned(),
            dns_independent: probe.bootstrap_dns_independent(),
            path,
            process,
            health: None,
            previous,
        });
        Ok(())
    }

    fn revision_health(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<DataPlaneCandidateHealth, PlatformVpnError> {
        let mut candidate = lock(&self.inner.candidate);
        if let Some(candidate) = candidate.as_mut() {
            if candidate.revision != revision {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
            if let Some(health) = candidate.health {
                return Ok(health);
            }
            let deadline = Instant::now() + HEALTH_TIMEOUT;
            let mut ready = false;
            while Instant::now() < deadline {
                if candidate.process.healthy()? {
                    ready = true;
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
            let reachable = ready
                && candidate
                    .process
                    .probe(&candidate.selector_id, &candidate.node_id, HEALTH_TIMEOUT)
                    .is_ok();
            let health = DataPlaneCandidateHealth::new(ready, reachable, candidate.dns_independent);
            candidate.health = Some(health);
            return Ok(health);
        }
        drop(candidate);

        if *lock(&self.inner.active) != Some(revision) {
            return Err(PlatformVpnError::Unavailable);
        }
        let snapshot = self.inner.adapter.snapshot()?;
        let config =
            inspect_runtime_config(&read_regular(&self.revision_path(revision), 1 << 20)?)?;
        let ready = snapshot.state() == DataPlaneState::Online && snapshot.has_active_instance();
        let reachable = ready
            && self
                .inner
                .adapter
                .probe_node_delay(
                    revision,
                    config.selector_id(),
                    config.default_node_id(),
                    HEALTH_TIMEOUT,
                    &CancellationToken::default(),
                )
                .is_ok();
        Ok(DataPlaneCandidateHealth::new(
            ready,
            reachable,
            config.bootstrap_dns_independent(),
        ))
    }

    fn activate_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        let candidate = lock(&self.inner.candidate)
            .take()
            .filter(|candidate| candidate.revision == revision)
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        if candidate
            .health
            .is_none_or(|health| health.failed_check().is_some())
        {
            let previous = candidate.previous;
            let path = candidate.path.clone();
            let _ = candidate.process.stop();
            let _ = remove_regular(&path);
            let _ = restore_runtime(&self.inner.adapter, previous);
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let previous = candidate.previous;
        let path = candidate.path.clone();
        candidate.process.stop()?;
        remove_regular(&path)?;
        restore_runtime(&self.inner.adapter, Some(revision))?;
        if let Err(error) = self.persist_active(Some(revision)) {
            let _ = restore_runtime(&self.inner.adapter, previous);
            return Err(error);
        }
        *lock(&self.inner.active) = Some(revision);
        Ok(())
    }

    fn active_revision(&self) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
        Ok(*lock(&self.inner.active))
    }

    fn public_catalog(
        &self,
    ) -> Result<Option<(ConfigurationRevision, SelectorCatalog)>, PlatformVpnError> {
        let revision = *lock(&self.inner.active);
        revision
            .map(|revision| {
                inspect_runtime_config(&read_regular(&self.revision_path(revision), 1 << 20)?)
                    .map(|config| (revision, config.catalog().clone()))
            })
            .transpose()
    }

    fn restore_active(
        &self,
        revision: Option<ConfigurationRevision>,
    ) -> Result<(), PlatformVpnError> {
        if let Some(candidate) = lock(&self.inner.candidate).take() {
            let path = candidate.path.clone();
            let _ = candidate.process.stop();
            remove_regular(&path)?;
        }
        let previous = *lock(&self.inner.active);
        restore_runtime(&self.inner.adapter, revision)?;
        if let Err(error) = self.persist_active(revision) {
            let _ = restore_runtime(&self.inner.adapter, previous);
            return Err(error);
        }
        *lock(&self.inner.active) = revision;
        Ok(())
    }

    fn discard_candidate(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        if *lock(&self.inner.active) == Some(revision) {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        if lock(&self.inner.install)
            .as_ref()
            .is_some_and(|state| state.revision == revision)
        {
            lock(&self.inner.install).take();
        }
        if let Some(candidate) = lock(&self.inner.candidate).take() {
            if candidate.revision != revision {
                *lock(&self.inner.candidate) = Some(candidate);
                return Err(PlatformVpnError::OperationInProgress);
            }
            let path = candidate.path.clone();
            let _ = candidate.process.stop();
            remove_regular(&path)?;
        }
        remove_regular(&self.temporary_path(revision))?;
        remove_regular(&self.probe_path(revision))?;
        remove_regular(&self.revision_path(revision))
    }
}

fn restore_runtime(
    adapter: &SupervisedVpnAdapter<MacosDataPlaneBackend>,
    revision: Option<ConfigurationRevision>,
) -> Result<(), PlatformVpnError> {
    let snapshot = adapter.snapshot()?;
    match revision {
        None if snapshot.has_active_instance() => adapter.stop(snapshot.instance_id()).map(drop),
        None => Ok(()),
        Some(revision) if snapshot.has_active_instance() => {
            adapter.restart(snapshot.instance_id(), revision)?;
            wait_online(adapter)
        }
        Some(revision) => {
            adapter.start(revision)?;
            wait_online(adapter)
        }
    }
}

fn wait_online(
    adapter: &SupervisedVpnAdapter<MacosDataPlaneBackend>,
) -> Result<(), PlatformVpnError> {
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        let snapshot = adapter.snapshot()?;
        match snapshot.state() {
            DataPlaneState::Online if snapshot.has_active_instance() => return Ok(()),
            DataPlaneState::PermissionRequired => return Err(PlatformVpnError::PermissionDenied),
            DataPlaneState::Failed | DataPlaneState::Unconfigured => {
                return Err(PlatformVpnError::Unavailable);
            }
            _ if Instant::now() >= deadline => return Err(PlatformVpnError::Timeout),
            _ => thread::sleep(Duration::from_millis(25)),
        }
    }
}

fn load_active(root: &Path) -> Result<Option<ConfigurationRevision>, PlatformVpnError> {
    let path = root.join(ACTIVE_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(PlatformVpnError::Unavailable),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != 0
        || metadata.len() == 0
        || metadata.len() > 20
    {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let value = fs::read_to_string(path).map_err(|_| PlatformVpnError::Unavailable)?;
    let revision = value
        .parse::<u64>()
        .ok()
        .and_then(|value| ConfigurationRevision::new(value).ok())
        .ok_or(PlatformVpnError::InvalidConfiguration)?;
    read_regular(&root.join(format!("{}.json", revision.get())), 1 << 20)?;
    Ok(Some(revision))
}

fn persist_active(
    root: &Path,
    revision: Option<ConfigurationRevision>,
) -> Result<(), PlatformVpnError> {
    let path = root.join(ACTIVE_FILE);
    let temporary = root.join(ACTIVE_TEMP);
    remove_regular(&temporary)?;
    let Some(revision) = revision else {
        return remove_regular(&path);
    };
    read_regular(&root.join(format!("{}.json", revision.get())), 1 << 20)?;
    write_new(&temporary, revision.get().to_string().as_bytes())?;
    fs::rename(&temporary, &path).map_err(|_| PlatformVpnError::Unavailable)?;
    sync_directory(root)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), PlatformVpnError> {
    remove_regular(path)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| PlatformVpnError::Unavailable)?;
    file.write_all(bytes)
        .map_err(|_| PlatformVpnError::Unavailable)?;
    file.sync_all().map_err(|_| PlatformVpnError::Unavailable)
}

fn read_regular(path: &Path, max: usize) -> Result<Vec<u8>, PlatformVpnError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| PlatformVpnError::Unavailable)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != 0
        || metadata.len() == 0
        || metadata.len() > max as u64
    {
        return Err(PlatformVpnError::PermissionDenied);
    }
    fs::read(path).map_err(|_| PlatformVpnError::Unavailable)
}

fn remove_regular(path: &Path) -> Result<(), PlatformVpnError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(PlatformVpnError::Unavailable),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.uid() != 0 {
        return Err(PlatformVpnError::PermissionDenied);
    }
    fs::remove_file(path).map_err(|_| PlatformVpnError::Unavailable)
}

fn sync_directory(path: &Path) -> Result<(), PlatformVpnError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| PlatformVpnError::Unavailable)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
