use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    net::TcpStream,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use core_foundation::url::CFURL;
use orange_platform::{
    CancellationToken, ConfigurationRevision, DataPlaneLifecycleBackend, DataPlaneNodeBackend,
    DelayProbeError, NodeBackendError, PINNED_SING_BOX_VERSION, PlatformVpnError, ProcessReadiness,
    SupervisedDataPlaneProcess, TrafficCounters,
};
use orange_service_core::{
    ManagedHostClient, ManagedHostController, ManagedInboundKind, inspect_runtime_config,
};
use security_framework::os::macos::code_signing::{Flags, SecRequirement, SecStaticCode};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{DEFAULT_DATA_PLANE_PATH, DEFAULT_STATE_ROOT, system_proxy::SystemProxyManager};

const MANIFEST: &[u8] = include_bytes!("../../../native/macos/data-plane-runtime-manifest.json");
const MANIFEST_SCHEMA_VERSION: u16 = 1;
const MAX_VERSION_OUTPUT: usize = 64 * 1024;
const PROXY_PORT: u16 = 24_836;
const PROBE_PORT: u16 = 24_837;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECTION_RECOVERY_FILE: &str = "connection-active.v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeManifest {
    schema_version: u16,
    artifact: Artifact,
    revision_store: RevisionStore,
    rules_path: String,
    release_allowed: bool,
    runtime_download_allowed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    runtime_path: String,
    sha256: String,
    version: String,
    go_compiler: String,
    build_tags: Vec<String>,
    bundle_identifier: String,
    team_identifier: String,
    universal2: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionStore {
    path: String,
    max_config_bytes: usize,
}

impl RuntimeManifest {
    fn embedded() -> Result<Self, PlatformVpnError> {
        let manifest: Self =
            serde_json::from_slice(MANIFEST).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        if manifest.schema_version != MANIFEST_SCHEMA_VERSION
            || manifest.artifact.runtime_path != DEFAULT_DATA_PLANE_PATH
            || manifest.artifact.version != PINNED_SING_BOX_VERSION
            || manifest.artifact.go_compiler != "go1.25.5"
            || manifest.artifact.build_tags != ["with_quic", "with_utls"]
            || manifest.artifact.bundle_identifier != "com.orangevpn.cn.data-plane"
            || !valid_team_id(&manifest.artifact.team_identifier)
            || !manifest.artifact.universal2
            || !valid_sha256(&manifest.artifact.sha256)
            || manifest.revision_store.path != "/Library/Application Support/Orange/revisions"
            || manifest.revision_store.max_config_bytes != 1 << 20
            || manifest.rules_path != "/Library/Application Support/Orange/rules"
            || manifest.runtime_download_allowed
            || !manifest.release_allowed
        {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(manifest)
    }

    fn signing_requirement(&self) -> Result<SecRequirement, PlatformVpnError> {
        format!(
            "identifier \"{}\" and anchor apple generic and certificate leaf[subject.OU] = \"{}\"",
            self.artifact.bundle_identifier, self.artifact.team_identifier
        )
        .parse()
        .map_err(|_| PlatformVpnError::InvalidConfiguration)
    }
}

#[derive(Clone)]
pub struct MacosDataPlaneBackend {
    inner: Arc<BackendInner>,
    controller: ManagedHostController,
}

struct BackendInner {
    manifest: RuntimeManifest,
    proxy: Arc<SystemProxyManager>,
    prepared: Mutex<Option<PreparedRevision>>,
}

struct PreparedRevision {
    revision: ConfigurationRevision,
    config: PathBuf,
    config_hash: String,
    inbound: ManagedInboundKind,
    preexisting_utuns: BTreeSet<String>,
}

impl MacosDataPlaneBackend {
    pub fn installed(proxy: Arc<SystemProxyManager>) -> Result<Self, PlatformVpnError> {
        Ok(Self {
            inner: Arc::new(BackendInner {
                manifest: RuntimeManifest::embedded()?,
                proxy,
                prepared: Mutex::new(None),
            }),
            controller: ManagedHostController::default(),
        })
    }

    pub fn revision_root(&self) -> &Path {
        Path::new(&self.inner.manifest.revision_store.path)
    }

    pub fn rules_root(&self) -> &Path {
        Path::new(&self.inner.manifest.rules_path)
    }

    pub fn connection_recovery_requested(&self) -> Result<bool, PlatformVpnError> {
        let Some(owner) = self.connection_recovery_owner()? else {
            return Ok(false);
        };
        Ok(console_user_uid() == Some(owner))
    }

    pub fn connection_recovery_owner(&self) -> Result<Option<u32>, PlatformVpnError> {
        let path = connection_recovery_path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(PlatformVpnError::Unavailable),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.len() == 0
            || metadata.len() > 32
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let bytes = fs::read(&path).map_err(|_| PlatformVpnError::Unavailable)?;
        let value = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|value| value.strip_prefix("uid="))
            .and_then(|value| value.strip_suffix('\n'))
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|uid| *uid != 0)
            .ok_or(PlatformVpnError::PermissionDenied)?;
        Ok(Some(value))
    }

    pub fn clear_connection_recovery(&self) -> Result<(), PlatformVpnError> {
        clear_connection_active()
    }

    pub fn start_probe(
        &self,
        revision: ConfigurationRevision,
        config: &Path,
    ) -> Result<CandidateProcess, PlatformVpnError> {
        self.verify_artifact()?;
        let bytes = read_regular(config, 1 << 20)?;
        let inspected = inspect_runtime_config(&bytes)?;
        if inspected.inbound() != ManagedInboundKind::Probe {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        let process = MacosSidecarProcess::spawn(
            Path::new(DEFAULT_DATA_PLANE_PATH),
            config,
            ManagedInboundKind::Probe,
            BTreeSet::new(),
            Arc::clone(&self.inner.proxy),
        )?;
        CandidateProcess::new(revision, process)
    }

    fn verify_artifact(&self) -> Result<(), PlatformVpnError> {
        let path = Path::new(DEFAULT_DATA_PLANE_PATH);
        let metadata = fs::symlink_metadata(path).map_err(|_| PlatformVpnError::Unavailable)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || metadata.permissions().mode() & 0o022 != 0
            || sha256_path(path, Some(128 * 1024 * 1024))? != self.inner.manifest.artifact.sha256
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let url = CFURL::from_path(path, false).ok_or(PlatformVpnError::PermissionDenied)?;
        let code = SecStaticCode::from_path(&url, Flags::NONE)
            .map_err(|_| PlatformVpnError::PermissionDenied)?;
        code.check_validity(
            Flags::CHECK_ALL_ARCHITECTURES | Flags::STRICT_VALIDATE | Flags::NO_NETWORK_ACCESS,
            &self.inner.manifest.signing_requirement()?,
        )
        .map_err(|_| PlatformVpnError::PermissionDenied)
    }

    fn verify_version(&self) -> Result<(), PlatformVpnError> {
        let output = run_bounded(Path::new(DEFAULT_DATA_PLANE_PATH), &["version"])?;
        let expected = format!(
            "sing-box version {}\n\nEnvironment: {} darwin/",
            self.inner.manifest.artifact.version, self.inner.manifest.artifact.go_compiler
        );
        if output.starts_with(&expected)
            && output.contains("\nTags: with_quic,with_utls\nCGO: disabled\n")
            && (output.contains("darwin/amd64\n") || output.contains("darwin/arm64\n"))
        {
            Ok(())
        } else {
            Err(PlatformVpnError::PermissionDenied)
        }
    }
}

impl DataPlaneLifecycleBackend for MacosDataPlaneBackend {
    type Process = MacosSidecarProcess;

    fn preflight(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        *lock(&self.inner.prepared) = None;
        self.verify_artifact()?;
        self.verify_version()?;
        let config = fixed_revision_path(self.revision_root(), revision)?;
        let bytes = read_regular(&config, self.inner.manifest.revision_store.max_config_bytes)?;
        let inspected = inspect_runtime_config(&bytes)?;
        run_bounded(
            Path::new(DEFAULT_DATA_PLANE_PATH),
            &[
                "check",
                "-c",
                config
                    .to_str()
                    .ok_or(PlatformVpnError::InvalidConfiguration)?,
            ],
        )?;
        self.verify_artifact()?;
        let preexisting_utuns = if inspected.inbound() == ManagedInboundKind::Tun {
            utun_names()?
        } else {
            BTreeSet::new()
        };
        *lock(&self.inner.prepared) = Some(PreparedRevision {
            revision,
            config,
            config_hash: format!("{:x}", Sha256::digest(&bytes)),
            inbound: inspected.inbound(),
            preexisting_utuns,
        });
        Ok(())
    }

    fn spawn(
        &self,
        revision: ConfigurationRevision,
        instance_id: u64,
    ) -> Result<Self::Process, PlatformVpnError> {
        let prepared = lock(&self.inner.prepared)
            .take()
            .filter(|prepared| prepared.revision == revision)
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        self.verify_artifact()?;
        let bytes = read_regular(&prepared.config, 1 << 20)?;
        if format!("{:x}", Sha256::digest(&bytes)) != prepared.config_hash {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let process = MacosSidecarProcess::spawn(
            Path::new(DEFAULT_DATA_PLANE_PATH),
            &prepared.config,
            prepared.inbound,
            prepared.preexisting_utuns,
            Arc::clone(&self.inner.proxy),
        )?;
        self.controller
            .activate(
                revision,
                instance_id,
                process.process_id(),
                process.client(),
            )
            .map_err(|_| PlatformVpnError::OperationInProgress)?;
        if let Err(error) = mark_connection_active() {
            self.controller.deactivate(instance_id);
            return Err(error);
        }
        Ok(process)
    }

    fn cleanup(&self, instance_id: u64) -> Result<(), PlatformVpnError> {
        self.controller.deactivate(instance_id);
        clear_connection_active()?;
        self.inner
            .proxy
            .restore()
            .map_err(|_| PlatformVpnError::CleanupFailed)
    }
}

impl DataPlaneNodeBackend for MacosDataPlaneBackend {
    fn select_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
    ) -> Result<(), NodeBackendError> {
        self.controller.select_node(revision, selector_id, node_id)
    }

    fn read_selected_node(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
    ) -> Result<String, NodeBackendError> {
        self.controller.read_selected_node(revision, selector_id)
    }

    fn probe_node_delay(
        &self,
        revision: ConfigurationRevision,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<u32, DelayProbeError> {
        self.controller
            .probe_node_delay(revision, selector_id, node_id, timeout, cancellation)
    }

    fn traffic_counters(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<TrafficCounters, NodeBackendError> {
        self.controller.traffic_counters(revision)
    }
}

pub struct CandidateProcess {
    revision: ConfigurationRevision,
    controller: ManagedHostController,
    process: MacosSidecarProcess,
}

impl CandidateProcess {
    fn new(
        revision: ConfigurationRevision,
        process: MacosSidecarProcess,
    ) -> Result<Self, PlatformVpnError> {
        let controller = ManagedHostController::default();
        controller
            .activate(revision, 1, process.process_id(), process.client())
            .map_err(|_| PlatformVpnError::Unavailable)?;
        Ok(Self {
            revision,
            controller,
            process,
        })
    }

    pub fn healthy(&mut self) -> Result<bool, PlatformVpnError> {
        if self.process.try_wait()? {
            return Ok(false);
        }
        Ok(matches!(self.process.readiness()?, ProcessReadiness::Ready))
    }

    pub fn probe(
        &self,
        selector_id: &str,
        node_id: &str,
        timeout: Duration,
    ) -> Result<u32, DelayProbeError> {
        self.controller.probe_node_delay(
            self.revision,
            selector_id,
            node_id,
            timeout,
            &CancellationToken::default(),
        )
    }

    pub fn stop(mut self) -> Result<(), PlatformVpnError> {
        self.controller.deactivate(1);
        self.process.force_stop()
    }
}

pub struct MacosSidecarProcess {
    child: Child,
    client: Arc<ManagedHostClient>,
    inbound: ManagedInboundKind,
    preexisting_utuns: BTreeSet<String>,
    managed_utun: Option<String>,
    proxy: Arc<SystemProxyManager>,
    proxy_applied: bool,
    reaped: bool,
}

impl MacosSidecarProcess {
    fn spawn(
        artifact: &Path,
        config: &Path,
        inbound: ManagedInboundKind,
        preexisting_utuns: BTreeSet<String>,
        proxy: Arc<SystemProxyManager>,
    ) -> Result<Self, PlatformVpnError> {
        let mut child = Command::new(artifact)
            .args(["run", "-c"])
            .arg(config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .spawn()
            .map_err(|_| PlatformVpnError::Unavailable)?;
        let stdin = child.stdin.take().ok_or(PlatformVpnError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(PlatformVpnError::Unavailable)?;
        let client = ManagedHostClient::connect(stdin, stdout).map_err(|_| {
            let _ = child.kill();
            let _ = child.wait();
            PlatformVpnError::ProtocolViolation
        })?;
        Ok(Self {
            child,
            client,
            inbound,
            preexisting_utuns,
            managed_utun: None,
            proxy,
            proxy_applied: false,
            reaped: false,
        })
    }

    fn client(&self) -> Arc<ManagedHostClient> {
        Arc::clone(&self.client)
    }
}

impl SupervisedDataPlaneProcess for MacosSidecarProcess {
    fn process_id(&self) -> u32 {
        self.child.id()
    }

    fn try_wait(&mut self) -> Result<bool, PlatformVpnError> {
        if self.reaped {
            return Ok(true);
        }
        let exited = self
            .child
            .try_wait()
            .map_err(|_| PlatformVpnError::Unavailable)?
            .is_some();
        self.reaped = exited;
        Ok(exited)
    }

    fn readiness(&mut self) -> Result<ProcessReadiness, PlatformVpnError> {
        if self.try_wait()? {
            return Err(PlatformVpnError::Crashed);
        }
        match self.inbound {
            ManagedInboundKind::Probe => {
                Ok(if TcpStream::connect(("127.0.0.1", PROBE_PORT)).is_ok() {
                    ProcessReadiness::Ready
                } else {
                    ProcessReadiness::Pending
                })
            }
            ManagedInboundKind::SystemProxy => {
                if TcpStream::connect(("127.0.0.1", PROXY_PORT)).is_err() {
                    return Ok(ProcessReadiness::Pending);
                }
                if !self.proxy_applied {
                    self.proxy
                        .ensure_applied()
                        .map_err(|_| PlatformVpnError::CleanupFailed)?;
                    self.proxy_applied = true;
                }
                Ok(ProcessReadiness::Ready)
            }
            ManagedInboundKind::Tun => {
                let current = utun_names()?;
                let new = current.difference(&self.preexisting_utuns).next().cloned();
                if let Some(name) = new {
                    if utun_has_fixed_addresses(&name)?
                        && utun_routes_ready(&name)?
                        && dns_resolvers_ready()?
                    {
                        self.managed_utun = Some(name);
                        return Ok(ProcessReadiness::Ready);
                    }
                }
                Ok(ProcessReadiness::Pending)
            }
        }
    }

    fn request_stop(&mut self) -> Result<(), PlatformVpnError> {
        self.client.close();
        Ok(())
    }

    fn force_stop(&mut self) -> Result<(), PlatformVpnError> {
        if !self.reaped {
            self.client.close();
            if self
                .child
                .try_wait()
                .map_err(|_| PlatformVpnError::Unavailable)?
                .is_none()
            {
                self.child
                    .kill()
                    .map_err(|_| PlatformVpnError::Unavailable)?;
            }
            self.child
                .wait()
                .map_err(|_| PlatformVpnError::Unavailable)?;
            self.reaped = true;
        }
        self.proxy
            .restore()
            .map_err(|_| PlatformVpnError::CleanupFailed)?;
        if let Some(name) = self.managed_utun.take() {
            wait_for_tun_cleanup(&name)?;
        }
        Ok(())
    }
}

impl Drop for MacosSidecarProcess {
    fn drop(&mut self) {
        let _ = self.force_stop();
    }
}

fn fixed_revision_path(
    root: &Path,
    revision: ConfigurationRevision,
) -> Result<PathBuf, PlatformVpnError> {
    let root = root
        .canonicalize()
        .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    let path = root.join(format!("{}.json", revision.get()));
    let canonical = path
        .canonicalize()
        .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    if canonical.parent() != Some(root.as_path()) {
        return Err(PlatformVpnError::PermissionDenied);
    }
    Ok(canonical)
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

fn sha256_path(path: &Path, limit: Option<u64>) -> Result<String, PlatformVpnError> {
    let mut file = File::open(path).map_err(|_| PlatformVpnError::Unavailable)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        if limit.is_some_and(|limit| total > limit) {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn run_bounded(program: &Path, args: &[&str]) -> Result<String, PlatformVpnError> {
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| PlatformVpnError::Unavailable)?;
    let stdout = child.stdout.take().ok_or(PlatformVpnError::Unavailable)?;
    let stderr = child.stderr.take().ok_or(PlatformVpnError::Unavailable)?;
    let stdout_reader = thread::spawn(move || read_limited(stdout, MAX_VERSION_OUTPUT));
    let stderr_reader = thread::spawn(move || read_limited(stderr, MAX_VERSION_OUTPUT));
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| PlatformVpnError::Unavailable)?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(PlatformVpnError::Timeout);
        }
        thread::sleep(Duration::from_millis(25));
    };
    let output = stdout_reader
        .join()
        .map_err(|_| PlatformVpnError::Unavailable)??;
    let error = stderr_reader
        .join()
        .map_err(|_| PlatformVpnError::Unavailable)??;
    if !status.success() || !error.is_empty() || output.len() > MAX_VERSION_OUTPUT {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    String::from_utf8(output).map_err(|_| PlatformVpnError::ProtocolViolation)
}

fn read_limited(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, PlatformVpnError> {
    let mut output = Vec::new();
    reader
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut output)
        .map_err(|_| PlatformVpnError::Unavailable)?;
    Ok(output)
}

fn connection_recovery_path() -> PathBuf {
    Path::new(DEFAULT_STATE_ROOT).join(CONNECTION_RECOVERY_FILE)
}

fn mark_connection_active() -> Result<(), PlatformVpnError> {
    let uid = console_user_uid().ok_or(PlatformVpnError::PermissionDenied)?;
    let contents = format!("uid={uid}\n");
    let path = connection_recovery_path();
    let temporary = path.with_extension("installing");
    let _ = remove_root_regular(&temporary);
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| PlatformVpnError::Unavailable)?;
    file.write_all(contents.as_bytes())
        .map_err(|_| PlatformVpnError::Unavailable)?;
    file.sync_all().map_err(|_| PlatformVpnError::Unavailable)?;
    drop(file);
    fs::rename(&temporary, &path).map_err(|_| PlatformVpnError::Unavailable)
}

fn console_user_uid() -> Option<u32> {
    let uid = fs::metadata("/dev/console").ok()?.uid();
    (uid != 0).then_some(uid)
}

fn clear_connection_active() -> Result<(), PlatformVpnError> {
    remove_root_regular(&connection_recovery_path())
}

fn remove_root_regular(path: &Path) -> Result<(), PlatformVpnError> {
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

fn utun_names() -> Result<BTreeSet<String>, PlatformVpnError> {
    let mut interfaces = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut interfaces) } != 0 {
        return Err(PlatformVpnError::Unavailable);
    }
    let mut names = BTreeSet::new();
    let mut cursor = interfaces;
    while !cursor.is_null() {
        let name = unsafe { std::ffi::CStr::from_ptr((*cursor).ifa_name) };
        if let Ok(name) = name.to_str()
            && name.starts_with("utun")
        {
            names.insert(name.to_owned());
        }
        cursor = unsafe { (*cursor).ifa_next };
    }
    unsafe { libc::freeifaddrs(interfaces) };
    Ok(names)
}

fn utun_has_fixed_addresses(name: &str) -> Result<bool, PlatformVpnError> {
    let output = Command::new("/sbin/ifconfig")
        .arg(name)
        .output()
        .map_err(|_| PlatformVpnError::Unavailable)?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Ok(false);
    }
    let value =
        String::from_utf8(output.stdout).map_err(|_| PlatformVpnError::ProtocolViolation)?;
    Ok(value.contains("172.19.0.1") && value.contains("fdfe:dcba:9876::1"))
}

fn utun_routes_ready(name: &str) -> Result<bool, PlatformVpnError> {
    let ipv4 = command_text("/usr/sbin/netstat", &["-rn", "-f", "inet"])?;
    let ipv6 = command_text("/usr/sbin/netstat", &["-rn", "-f", "inet6"])?;
    Ok(route_table_has_interface(&ipv4, name) && route_table_has_interface(&ipv6, name))
}

fn dns_resolvers_ready() -> Result<bool, PlatformVpnError> {
    let output = command_text("/usr/sbin/scutil", &["--dns"])?;
    Ok(output.contains("resolver #") && output.contains("nameserver["))
}

fn wait_for_tun_cleanup(name: &str) -> Result<(), PlatformVpnError> {
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while Instant::now() < deadline {
        let interface_absent = !utun_names()?.contains(name);
        let ipv4 = command_text("/usr/sbin/netstat", &["-rn", "-f", "inet"])?;
        let ipv6 = command_text("/usr/sbin/netstat", &["-rn", "-f", "inet6"])?;
        if interface_absent
            && !route_table_has_interface(&ipv4, name)
            && !route_table_has_interface(&ipv6, name)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(PlatformVpnError::CleanupFailed)
}

fn route_table_has_interface(table: &str, name: &str) -> bool {
    table.lines().any(|line| {
        line.split_ascii_whitespace()
            .last()
            .is_some_and(|interface| interface == name)
    })
}

fn command_text(program: &str, args: &[&str]) -> Result<String, PlatformVpnError> {
    let output = Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .output()
        .map_err(|_| PlatformVpnError::Unavailable)?;
    if !output.status.success()
        || !output.stderr.is_empty()
        || output.stdout.len() > MAX_VERSION_OUTPUT
    {
        return Err(PlatformVpnError::Unavailable);
    }
    String::from_utf8(output.stdout).map_err(|_| PlatformVpnError::ProtocolViolation)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_team_id(value: &str) -> bool {
    value.len() == 10
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::route_table_has_interface;

    #[test]
    fn route_table_requires_interface_column_match() {
        let table = "Destination Gateway Flags Netif Expire\ndefault link#22 UCSg utun7\n";
        assert!(route_table_has_interface(table, "utun7"));
        assert!(!route_table_has_interface(table, "utun"));
        assert!(!route_table_has_interface(table, "utun8"));
    }
}
