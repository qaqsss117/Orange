use std::{
    collections::HashSet,
    ffi::{OsStr, OsString, c_void},
    fs::File,
    io::Read,
    mem::size_of,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::AsRawHandle,
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    ptr,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use orange_platform::{
    CancellationToken, ConfigurationRevision, DataPlaneLifecycleBackend, DataPlaneNodeBackend,
    DelayProbeError, MAX_SUBSCRIPTION_CONFIG_BYTES, NodeBackendError, PINNED_SING_BOX_VERSION,
    PlatformVpnError, ProcessReadiness, SupervisedDataPlaneProcess, TrafficCounters,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_BUFFER_OVERFLOW, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_DATA, HANDLE,
        INVALID_HANDLE_VALUE, NO_ERROR,
    },
    NetworkManagement::{
        IpHelper::{
            GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
            GetAdaptersAddresses, GetExtendedTcpTable, IP_ADAPTER_ADDRESSES_LH,
            IP_ADAPTER_UNICAST_ADDRESS_LH, MIB_TCP_STATE_LISTEN, MIB_TCPROW_OWNER_PID,
            TCP_TABLE_OWNER_PID_LISTENER,
        },
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6},
    Security::{
        Cryptography::{CERT_SHA1_HASH_PROP_ID, CertGetCertificateContextProperty},
        WinTrust::{
            WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
            WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_DISABLE_MD2_MD4,
            WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE,
            WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTD_UICONTEXT_EXECUTE,
            WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
            WTHelperProvDataFromStateData, WinVerifyTrust,
        },
    },
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        SystemInformation::GetWindowsDirectoryW,
    },
};
use zeroize::Zeroizing;

use crate::managed_host::{ManagedHostClient, ManagedHostController};

const RUNTIME_MANIFEST_BYTES: &[u8] =
    include_bytes!("../../../native/windows/data-plane-runtime-manifest.json");
const RUNTIME_MANIFEST_SCHEMA_VERSION: u16 = 1;
const FIXED_ARTIFACT_PATH: &str = "orange-data-plane.exe";
const FIXED_REVISION_ROOT: &str = "data-plane/revisions";
const FIXED_REVISION_SUFFIX: &str = ".json";
const FIXED_GO_COMPILER: &str = "go1.25.5";
const FIXED_GOOS: &str = "windows";
const FIXED_GOARCH: &str = "amd64";
const FIXED_BUILD_TAGS: [&str; 2] = ["with_quic", "with_utls"];
const SHA256_HEX_BYTES: usize = 64;
const SHA1_HEX_BYTES: usize = 40;
const MAX_HANDSHAKE_OUTPUT_BYTES: u64 = 64 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const TUN_INTERFACE_NAME: &str = "orange-tun";
const TUN_IPV4_ADDRESS: Ipv4Addr = Ipv4Addr::new(172, 19, 0, 1);
const TUN_IPV4_PREFIX_LENGTH: u8 = 30;
const TUN_IPV6_ADDRESS: Ipv6Addr = Ipv6Addr::new(0xfdfe, 0xdcba, 0x9876, 0, 0, 0, 0, 1);
const TUN_IPV6_PREFIX_LENGTH: u8 = 126;
const TUN_CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const TUN_PROBE_INTERVAL: Duration = Duration::from_millis(25);
const SYSTEM_PROXY_LISTEN_PORT: u16 = 24836;
const CANDIDATE_LISTEN_PORT: u16 = 24837;
const INITIAL_ADAPTER_BUFFER_BYTES: u32 = 15 * 1024;
const MAX_ADAPTER_BUFFER_BYTES: u32 = 1024 * 1024;
const MAX_ADAPTER_QUERY_ATTEMPTS: usize = 3;
const MAX_TCP_TABLE_BUFFER_BYTES: u32 = 4 * 1024 * 1024;
const MAX_TCP_TABLE_QUERY_ATTEMPTS: usize = 3;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeManifest {
    schema_version: u16,
    artifact: ArtifactManifest,
    revision_store: RevisionStoreManifest,
    runtime_download_allowed: bool,
    release_allowed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactManifest {
    runtime_relative_path: String,
    sha256: String,
    version: String,
    go_compiler: String,
    target: ArtifactTarget,
    build_tags: Vec<String>,
    authenticode_required: bool,
    allowed_signer_sha1_thumbprints: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactTarget {
    goos: String,
    goarch: String,
    cgo_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionStoreManifest {
    relative_path: String,
    file_suffix: String,
    max_config_bytes: usize,
}

impl RuntimeManifest {
    fn embedded() -> Result<Self, PlatformVpnError> {
        let manifest: Self = serde_json::from_slice(RUNTIME_MANIFEST_BYTES)
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), PlatformVpnError> {
        if self.schema_version != RUNTIME_MANIFEST_SCHEMA_VERSION
            || self.artifact.runtime_relative_path != FIXED_ARTIFACT_PATH
            || self.artifact.version != PINNED_SING_BOX_VERSION
            || self.artifact.go_compiler != FIXED_GO_COMPILER
            || self.artifact.target.goos != FIXED_GOOS
            || self.artifact.target.goarch != FIXED_GOARCH
            || self.artifact.target.cgo_enabled
            || self.artifact.build_tags != FIXED_BUILD_TAGS
            || !self.artifact.authenticode_required
            || self.revision_store.relative_path != FIXED_REVISION_ROOT
            || self.revision_store.file_suffix != FIXED_REVISION_SUFFIX
            || self.revision_store.max_config_bytes != MAX_SUBSCRIPTION_CONFIG_BYTES
            || self.runtime_download_allowed
            || !is_lower_hex(&self.artifact.sha256, SHA256_HEX_BYTES)
        {
            return Err(PlatformVpnError::InvalidConfiguration);
        }

        let mut signers = HashSet::new();
        for signer in &self.artifact.allowed_signer_sha1_thumbprints {
            if !is_upper_hex(signer, SHA1_HEX_BYTES) || !signers.insert(signer) {
                return Err(PlatformVpnError::InvalidConfiguration);
            }
        }
        if self.release_allowed && signers.is_empty() {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(())
    }

    fn verify_version_output(&self, output: &str) -> Result<(), PlatformVpnError> {
        let normalized = output.replace("\r\n", "\n");
        if normalized.contains('\r') {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        let body = normalized.strip_suffix('\n').unwrap_or(&normalized);
        let expected = format!(
            "sing-box version {}\n\nEnvironment: {} {}/{}\nTags: {}\nCGO: disabled",
            self.artifact.version,
            self.artifact.go_compiler,
            self.artifact.target.goos,
            self.artifact.target.goarch,
            self.artifact.build_tags.join(",")
        );
        if body == expected {
            Ok(())
        } else {
            Err(PlatformVpnError::PermissionDenied)
        }
    }

    fn signer_allowed(&self, signer: &str) -> bool {
        self.artifact
            .allowed_signer_sha1_thumbprints
            .iter()
            .any(|allowed| allowed == signer)
    }
}

fn is_lower_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_upper_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
}

#[derive(Debug, Clone)]
struct RuntimeLayout {
    installation_root: PathBuf,
}

impl RuntimeLayout {
    fn new(installation_root: impl AsRef<Path>) -> Result<Self, PlatformVpnError> {
        let installation_root = installation_root
            .as_ref()
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        if !installation_root.is_absolute() || !installation_root.is_dir() {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(Self { installation_root })
    }

    fn artifact(&self) -> Result<PathBuf, PlatformVpnError> {
        let artifact = self
            .installation_root
            .join(FIXED_ARTIFACT_PATH)
            .canonicalize()
            .map_err(|_| PlatformVpnError::Unavailable)?;
        if !artifact.is_file()
            || artifact
                .parent()
                .is_none_or(|parent| !same_path(parent, &self.installation_root))
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        Ok(artifact)
    }

    fn revision_config(
        &self,
        revision: ConfigurationRevision,
        manifest: &RuntimeManifest,
    ) -> Result<PathBuf, PlatformVpnError> {
        let revision_root = self
            .installation_root
            .join(&manifest.revision_store.relative_path)
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        let expected_parent = self.installation_root.join(FIXED_REVISION_ROOT);
        if !revision_root.is_dir() || !same_path(&revision_root, &expected_parent) {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let filename = format!("{}{}", revision.get(), manifest.revision_store.file_suffix);
        let config = revision_root
            .join(filename)
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        if !config.is_file()
            || config
                .parent()
                .is_none_or(|parent| !same_path(parent, &revision_root))
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let length = config
            .metadata()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?
            .len();
        if length == 0 || length > manifest.revision_store.max_config_bytes as u64 {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        Ok(config)
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

fn sha256_path(path: &Path, max_bytes: Option<u64>) -> Result<String, PlatformVpnError> {
    let before = path.metadata().map_err(|_| PlatformVpnError::Unavailable)?;
    if !before.is_file() || max_bytes.is_some_and(|limit| before.len() > limit) {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let mut file = File::open(path).map_err(|_| PlatformVpnError::Unavailable)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes_read = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| PlatformVpnError::Unavailable)?;
        if read == 0 {
            break;
        }
        bytes_read = bytes_read
            .checked_add(read as u64)
            .ok_or(PlatformVpnError::InvalidConfiguration)?;
        if max_bytes.is_some_and(|limit| bytes_read > limit) {
            return Err(PlatformVpnError::InvalidConfiguration);
        }
        digest.update(&buffer[..read]);
    }
    let after = path.metadata().map_err(|_| PlatformVpnError::Unavailable)?;
    if before.len() != after.len() || bytes_read != after.len() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    Ok(format!("{:x}", digest.finalize()))
}

trait SidecarTrustVerifier: Send + Sync + 'static {
    fn signer_sha1_thumbprint(&self, artifact: &Path) -> Result<String, PlatformVpnError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TunAddress {
    address: IpAddr,
    prefix_length: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TunInterfaceState {
    operational: bool,
    unicast_addresses: HashSet<TunAddress>,
}

impl TunInterfaceState {
    fn satisfies_contract(&self) -> bool {
        self.operational
            && self.unicast_addresses.contains(&TunAddress {
                address: IpAddr::V4(TUN_IPV4_ADDRESS),
                prefix_length: TUN_IPV4_PREFIX_LENGTH,
            })
            && self.unicast_addresses.contains(&TunAddress {
                address: IpAddr::V6(TUN_IPV6_ADDRESS),
                prefix_length: TUN_IPV6_PREFIX_LENGTH,
            })
    }
}

trait TunStateProbe: Send + Sync + 'static {
    fn orange_tun_state(&self) -> Result<Option<TunInterfaceState>, PlatformVpnError>;
}

fn tun_readiness(tun_probe: &dyn TunStateProbe) -> Result<ProcessReadiness, PlatformVpnError> {
    Ok(match tun_probe.orange_tun_state()?.as_ref() {
        Some(state) if state.satisfies_contract() => ProcessReadiness::Ready,
        _ => ProcessReadiness::Pending,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeReadiness {
    Tun,
    MixedLoopback { port: u16 },
}

#[derive(Deserialize)]
struct RuntimeReadinessDocument {
    inbounds: Vec<RuntimeReadinessInbound>,
}

#[derive(Deserialize)]
struct RuntimeReadinessInbound {
    #[serde(rename = "type")]
    kind: String,
    listen: Option<String>,
    listen_port: Option<u16>,
}

fn runtime_readiness(config: &Path) -> Result<RuntimeReadiness, PlatformVpnError> {
    let metadata = config
        .metadata()
        .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_SUBSCRIPTION_CONFIG_BYTES as u64
    {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(metadata.len() as usize));
    File::open(config)
        .map_err(|_| PlatformVpnError::InvalidConfiguration)?
        .take(MAX_SUBSCRIPTION_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PlatformVpnError::Unavailable)?;
    if bytes.len() != metadata.len() as usize {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let mut document: RuntimeReadinessDocument =
        serde_json::from_slice(&bytes).map_err(|_| PlatformVpnError::InvalidConfiguration)?;
    if document.inbounds.len() != 1 {
        return Err(PlatformVpnError::InvalidConfiguration);
    }
    let inbound = document.inbounds.pop().unwrap();
    match inbound.kind.as_str() {
        "tun" => Ok(RuntimeReadiness::Tun),
        "mixed"
            if inbound.listen.as_deref() == Some("127.0.0.1")
                && matches!(
                    inbound.listen_port,
                    Some(SYSTEM_PROXY_LISTEN_PORT) | Some(CANDIDATE_LISTEN_PORT)
                ) =>
        {
            Ok(RuntimeReadiness::MixedLoopback {
                port: inbound.listen_port.unwrap(),
            })
        }
        _ => Err(PlatformVpnError::InvalidConfiguration),
    }
}

fn mixed_listener_readiness(
    process_id: u32,
    port: u16,
) -> Result<ProcessReadiness, PlatformVpnError> {
    let expected_address = u32::from_ne_bytes(Ipv4Addr::LOCALHOST.octets());
    for _ in 0..MAX_TCP_TABLE_QUERY_ATTEMPTS {
        let mut required_bytes = 0_u32;
        let result = unsafe {
            GetExtendedTcpTable(
                ptr::null_mut(),
                &mut required_bytes,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if !matches!(result, NO_ERROR | ERROR_INSUFFICIENT_BUFFER)
            || required_bytes < size_of::<u32>() as u32
            || required_bytes > MAX_TCP_TABLE_BUFFER_BYTES
        {
            return Err(PlatformVpnError::Unavailable);
        }
        let word_count = (required_bytes as usize).div_ceil(size_of::<u32>());
        let mut buffer = vec![0_u32; word_count];
        let result = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut required_bytes,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if result != NO_ERROR {
            return Err(PlatformVpnError::Unavailable);
        }
        let row_count = buffer[0] as usize;
        let table_bytes = size_of::<u32>()
            .checked_add(
                row_count
                    .checked_mul(size_of::<MIB_TCPROW_OWNER_PID>())
                    .ok_or(PlatformVpnError::ProtocolViolation)?,
            )
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        if table_bytes > required_bytes as usize || table_bytes > buffer.len() * size_of::<u32>() {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        for index in 0..row_count {
            let offset = size_of::<u32>() + index * size_of::<MIB_TCPROW_OWNER_PID>();
            let row = unsafe {
                buffer
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset)
                    .cast::<MIB_TCPROW_OWNER_PID>()
                    .read_unaligned()
            };
            if row.dwState == MIB_TCP_STATE_LISTEN as u32
                && row.dwLocalAddr == expected_address
                && u16::from_be(row.dwLocalPort as u16) == port
                && row.dwOwningPid == process_id
            {
                return Ok(ProcessReadiness::Ready);
            }
        }
        return Ok(ProcessReadiness::Pending);
    }
    Err(PlatformVpnError::Unavailable)
}

#[derive(Debug, Clone, Copy)]
struct TunCleanupPolicy {
    timeout: Duration,
    poll_interval: Duration,
}

impl Default for TunCleanupPolicy {
    fn default() -> Self {
        Self {
            timeout: TUN_CLEANUP_TIMEOUT,
            poll_interval: TUN_PROBE_INTERVAL,
        }
    }
}

trait SidecarLauncher: Send + Sync + 'static {
    type Process: SupervisedDataPlaneProcess;

    fn version_output(&self, artifact: &Path, cwd: &Path) -> Result<String, PlatformVpnError>;
    fn check_config(
        &self,
        artifact: &Path,
        config: &Path,
        cwd: &Path,
    ) -> Result<(), PlatformVpnError>;
    fn spawn_run(
        &self,
        artifact: &Path,
        config: &Path,
        cwd: &Path,
        tun_probe: Arc<dyn TunStateProbe>,
    ) -> Result<Self::Process, PlatformVpnError>;
}

#[derive(Debug)]
struct PreparedRevision {
    revision: ConfigurationRevision,
    artifact: PathBuf,
    artifact_sha256: String,
    config: PathBuf,
    config_sha256: String,
}

struct BackendCore<V, L> {
    manifest: RuntimeManifest,
    layout: RuntimeLayout,
    verifier: V,
    launcher: L,
    tun_probe: Arc<dyn TunStateProbe>,
    cleanup_policy: TunCleanupPolicy,
    prepared: Mutex<Option<PreparedRevision>>,
}

impl<V, L> BackendCore<V, L>
where
    V: SidecarTrustVerifier,
    L: SidecarLauncher,
{
    fn new(
        installation_root: impl AsRef<Path>,
        manifest: RuntimeManifest,
        verifier: V,
        launcher: L,
        tun_probe: Arc<dyn TunStateProbe>,
    ) -> Result<Self, PlatformVpnError> {
        manifest.validate()?;
        Ok(Self {
            manifest,
            layout: RuntimeLayout::new(installation_root)?,
            verifier,
            launcher,
            tun_probe,
            cleanup_policy: TunCleanupPolicy::default(),
            prepared: Mutex::new(None),
        })
    }

    fn preflight_revision(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        *lock(&self.prepared) = None;
        self.require_tun_absent()?;
        let artifact = self.layout.artifact()?;
        let config = self.layout.revision_config(revision, &self.manifest)?;
        let config_limit = Some(self.manifest.revision_store.max_config_bytes as u64);
        let config_sha256 = sha256_path(&config, config_limit)?;
        let artifact_sha256 = sha256_path(&artifact, None)?;
        if artifact_sha256 != self.manifest.artifact.sha256 {
            return Err(PlatformVpnError::PermissionDenied);
        }

        let signer = self.verifier.signer_sha1_thumbprint(&artifact)?;
        if !self.manifest.signer_allowed(&signer) {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let output = self
            .launcher
            .version_output(&artifact, &self.layout.installation_root)?;
        self.manifest.verify_version_output(&output)?;
        if sha256_path(&artifact, None)? != artifact_sha256 {
            return Err(PlatformVpnError::PermissionDenied);
        }

        self.launcher
            .check_config(&artifact, &config, &self.layout.installation_root)?;
        if sha256_path(&artifact, None)? != artifact_sha256
            || sha256_path(&config, config_limit)? != config_sha256
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        *lock(&self.prepared) = Some(PreparedRevision {
            revision,
            artifact,
            artifact_sha256,
            config,
            config_sha256,
        });
        Ok(())
    }

    fn spawn_revision(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<L::Process, PlatformVpnError> {
        let prepared = lock(&self.prepared)
            .take()
            .filter(|prepared| prepared.revision == revision)
            .ok_or(PlatformVpnError::ProtocolViolation)?;
        let artifact = self.layout.artifact()?;
        let config = self.layout.revision_config(revision, &self.manifest)?;
        let config_limit = Some(self.manifest.revision_store.max_config_bytes as u64);
        if !same_path(&artifact, &prepared.artifact)
            || !same_path(&config, &prepared.config)
            || sha256_path(&artifact, None)? != prepared.artifact_sha256
            || sha256_path(&config, config_limit)? != prepared.config_sha256
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        self.require_tun_absent()?;
        self.launcher.spawn_run(
            &artifact,
            &config,
            &self.layout.installation_root,
            Arc::clone(&self.tun_probe),
        )
    }

    fn require_tun_absent(&self) -> Result<(), PlatformVpnError> {
        if self.tun_probe.orange_tun_state()?.is_some() {
            Err(PlatformVpnError::OperationInProgress)
        } else {
            Ok(())
        }
    }

    fn cleanup_tun(&self) -> Result<(), PlatformVpnError> {
        let deadline = Instant::now() + self.cleanup_policy.timeout;
        loop {
            match self.tun_probe.orange_tun_state() {
                Ok(None) => return Ok(()),
                Ok(Some(_)) if Instant::now() < deadline => {
                    thread::sleep(self.cleanup_policy.poll_interval);
                }
                Ok(Some(_)) | Err(_) => return Err(PlatformVpnError::CleanupFailed),
            }
        }
    }
}

impl<V, L> DataPlaneLifecycleBackend for BackendCore<V, L>
where
    V: SidecarTrustVerifier,
    L: SidecarLauncher,
{
    type Process = L::Process;

    fn preflight(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        self.preflight_revision(revision)
    }

    fn spawn(
        &self,
        revision: ConfigurationRevision,
        _instance_id: u64,
    ) -> Result<Self::Process, PlatformVpnError> {
        self.spawn_revision(revision)
    }

    fn cleanup(&self, _instance_id: u64) -> Result<(), PlatformVpnError> {
        self.cleanup_tun()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct WindowsDataPlaneBackend {
    inner: Arc<BackendCore<NativeTrustVerifier, NativeLauncher>>,
    controller: ManagedHostController,
}

impl Clone for WindowsDataPlaneBackend {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            controller: self.controller.clone(),
        }
    }
}

impl WindowsDataPlaneBackend {
    pub fn new(installation_root: impl AsRef<Path>) -> Result<Self, PlatformVpnError> {
        let tun_probe = Arc::new(NativeTunStateProbe);
        Ok(Self {
            inner: Arc::new(BackendCore::new(
                installation_root,
                RuntimeManifest::embedded()?,
                NativeTrustVerifier,
                NativeLauncher,
                tun_probe,
            )?),
            controller: ManagedHostController::default(),
        })
    }

    pub(crate) fn start_candidate_probe(
        &self,
        revision: ConfigurationRevision,
        config: &Path,
    ) -> Result<WindowsCandidateProbe, PlatformVpnError> {
        self.inner.require_tun_absent()?;
        let revision_root = self
            .inner
            .layout
            .installation_root
            .join(FIXED_REVISION_ROOT)
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        let config = config
            .canonicalize()
            .map_err(|_| PlatformVpnError::InvalidConfiguration)?;
        let expected_name = format!(".{}.probe.json", revision.get());
        if !config.is_file()
            || config
                .parent()
                .is_none_or(|parent| !same_path(parent, &revision_root))
            || config.file_name().and_then(OsStr::to_str) != Some(expected_name.as_str())
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let config_limit = Some(self.inner.manifest.revision_store.max_config_bytes as u64);
        let config_sha256 = sha256_path(&config, config_limit)?;
        let artifact = self.inner.layout.artifact()?;
        let artifact_sha256 = sha256_path(&artifact, None)?;
        if artifact_sha256 != self.inner.manifest.artifact.sha256 {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let signer = self.inner.verifier.signer_sha1_thumbprint(&artifact)?;
        if !self.inner.manifest.signer_allowed(&signer) {
            return Err(PlatformVpnError::PermissionDenied);
        }
        let output = self
            .inner
            .launcher
            .version_output(&artifact, &self.inner.layout.installation_root)?;
        self.inner.manifest.verify_version_output(&output)?;
        self.inner.launcher.check_config(
            &artifact,
            &config,
            &self.inner.layout.installation_root,
        )?;
        if sha256_path(&artifact, None)? != artifact_sha256
            || sha256_path(&config, config_limit)? != config_sha256
        {
            return Err(PlatformVpnError::PermissionDenied);
        }
        self.inner.require_tun_absent()?;
        let process = self.inner.launcher.spawn_run(
            &artifact,
            &config,
            &self.inner.layout.installation_root,
            Arc::clone(&self.inner.tun_probe),
        )?;
        WindowsCandidateProbe::new(revision, process)
    }
}

pub(crate) struct WindowsCandidateProbe {
    revision: ConfigurationRevision,
    controller: ManagedHostController,
    process: WindowsSidecarProcess,
}

impl WindowsCandidateProbe {
    fn new(
        revision: ConfigurationRevision,
        process: WindowsSidecarProcess,
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

    pub(crate) fn is_running(&mut self) -> Result<bool, PlatformVpnError> {
        self.process.try_wait().map(|exited| !exited)
    }

    pub(crate) fn probe_delay(
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

    pub(crate) fn stop(mut self) -> Result<(), PlatformVpnError> {
        self.controller.deactivate(1);
        self.process.request_stop()?;
        self.process.reap()
    }
}

impl DataPlaneLifecycleBackend for WindowsDataPlaneBackend {
    type Process = WindowsSidecarProcess;

    fn preflight(&self, revision: ConfigurationRevision) -> Result<(), PlatformVpnError> {
        self.inner.preflight_revision(revision)
    }

    fn spawn(
        &self,
        revision: ConfigurationRevision,
        instance_id: u64,
    ) -> Result<Self::Process, PlatformVpnError> {
        let process = self.inner.spawn_revision(revision)?;
        self.controller
            .activate(
                revision,
                instance_id,
                process.process_id(),
                process.client(),
            )
            .map_err(|_| PlatformVpnError::OperationInProgress)?;
        Ok(process)
    }

    fn cleanup(&self, instance_id: u64) -> Result<(), PlatformVpnError> {
        self.controller.deactivate(instance_id);
        self.inner.cleanup_tun()
    }
}

impl DataPlaneNodeBackend for WindowsDataPlaneBackend {
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

struct NativeTrustVerifier;

impl SidecarTrustVerifier for NativeTrustVerifier {
    fn signer_sha1_thumbprint(&self, artifact: &Path) -> Result<String, PlatformVpnError> {
        verify_authenticode_signer(artifact)
    }
}

struct NativeTunStateProbe;

impl TunStateProbe for NativeTunStateProbe {
    fn orange_tun_state(&self) -> Result<Option<TunInterfaceState>, PlatformVpnError> {
        query_orange_tun_state()
    }
}

fn query_orange_tun_state() -> Result<Option<TunInterfaceState>, PlatformVpnError> {
    let mut required_bytes = INITIAL_ADAPTER_BUFFER_BYTES;
    for _ in 0..MAX_ADAPTER_QUERY_ATTEMPTS {
        if required_bytes == 0 || required_bytes > MAX_ADAPTER_BUFFER_BYTES {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        let words = (required_bytes as usize).div_ceil(size_of::<usize>());
        let mut buffer = vec![0_usize; words];
        let status = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_UNSPEC),
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
                ptr::null(),
                buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
                &mut required_bytes,
            )
        };
        if status == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        if status == ERROR_NO_DATA {
            return Ok(None);
        }
        if status != NO_ERROR {
            return Err(PlatformVpnError::Unavailable);
        }
        return parse_orange_tun_state(&buffer);
    }
    Err(PlatformVpnError::Unavailable)
}

fn parse_orange_tun_state(buffer: &[usize]) -> Result<Option<TunInterfaceState>, PlatformVpnError> {
    let buffer_start = buffer.as_ptr().cast::<u8>();
    let buffer_bytes = buffer
        .len()
        .checked_mul(size_of::<usize>())
        .ok_or(PlatformVpnError::ProtocolViolation)?;
    let mut adapter = buffer_start.cast::<IP_ADAPTER_ADDRESSES_LH>().cast_mut();
    let mut visited = HashSet::new();
    let mut matching = None;

    while !adapter.is_null() {
        require_buffer_value(adapter, buffer_start, buffer_bytes)?;
        if !visited.insert(adapter as usize) {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        let current = unsafe { &*adapter };
        if wide_buffer_equals(
            current.FriendlyName,
            TUN_INTERFACE_NAME,
            buffer_start,
            buffer_bytes,
        )? {
            if matching.is_some() {
                return Err(PlatformVpnError::ProtocolViolation);
            }
            matching = Some(TunInterfaceState {
                operational: current.OperStatus == IfOperStatusUp,
                unicast_addresses: read_unicast_addresses(
                    current.FirstUnicastAddress,
                    buffer_start,
                    buffer_bytes,
                )?,
            });
        }
        adapter = current.Next;
    }
    Ok(matching)
}

fn read_unicast_addresses(
    mut address: *mut IP_ADAPTER_UNICAST_ADDRESS_LH,
    buffer_start: *const u8,
    buffer_bytes: usize,
) -> Result<HashSet<TunAddress>, PlatformVpnError> {
    let mut addresses = HashSet::new();
    let mut visited = HashSet::new();
    while !address.is_null() {
        require_buffer_value(address, buffer_start, buffer_bytes)?;
        if !visited.insert(address as usize) {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        let current = unsafe { &*address };
        let socket = current.Address;
        if socket.lpSockaddr.is_null() || socket.iSockaddrLength < size_of::<SOCKADDR>() as i32 {
            return Err(PlatformVpnError::ProtocolViolation);
        }
        require_buffer_value(socket.lpSockaddr, buffer_start, buffer_bytes)?;
        let family = unsafe { (*socket.lpSockaddr).sa_family };
        let parsed = if family == AF_INET {
            if socket.iSockaddrLength < size_of::<SOCKADDR_IN>() as i32 {
                return Err(PlatformVpnError::ProtocolViolation);
            }
            let ipv4 = socket.lpSockaddr.cast::<SOCKADDR_IN>();
            require_buffer_value(ipv4, buffer_start, buffer_bytes)?;
            let octets = unsafe { (*ipv4).sin_addr.S_un.S_un_b };
            Some(IpAddr::V4(Ipv4Addr::new(
                octets.s_b1,
                octets.s_b2,
                octets.s_b3,
                octets.s_b4,
            )))
        } else if family == AF_INET6 {
            if socket.iSockaddrLength < size_of::<SOCKADDR_IN6>() as i32 {
                return Err(PlatformVpnError::ProtocolViolation);
            }
            let ipv6 = socket.lpSockaddr.cast::<SOCKADDR_IN6>();
            require_buffer_value(ipv6, buffer_start, buffer_bytes)?;
            let octets = unsafe { (*ipv6).sin6_addr.u.Byte };
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        } else {
            None
        };
        if let Some(address) = parsed {
            addresses.insert(TunAddress {
                address,
                prefix_length: current.OnLinkPrefixLength,
            });
        }
        address = current.Next;
    }
    Ok(addresses)
}

fn wide_buffer_equals(
    value: *const u16,
    expected: &str,
    buffer_start: *const u8,
    buffer_bytes: usize,
) -> Result<bool, PlatformVpnError> {
    let expected = expected.encode_utf16().collect::<Vec<_>>();
    let units = expected
        .len()
        .checked_add(1)
        .ok_or(PlatformVpnError::ProtocolViolation)?;
    let bytes = units
        .checked_mul(size_of::<u16>())
        .ok_or(PlatformVpnError::ProtocolViolation)?;
    require_buffer_range(value.cast::<u8>(), bytes, buffer_start, buffer_bytes)?;
    let actual = unsafe { std::slice::from_raw_parts(value, units) };
    Ok(actual[..expected.len()] == expected && actual[expected.len()] == 0)
}

fn require_buffer_value<T>(
    value: *const T,
    buffer_start: *const u8,
    buffer_bytes: usize,
) -> Result<(), PlatformVpnError> {
    if !(value as usize).is_multiple_of(std::mem::align_of::<T>()) {
        return Err(PlatformVpnError::ProtocolViolation);
    }
    require_buffer_range(
        value.cast::<u8>(),
        size_of::<T>(),
        buffer_start,
        buffer_bytes,
    )
}

fn require_buffer_range(
    value: *const u8,
    value_bytes: usize,
    buffer_start: *const u8,
    buffer_bytes: usize,
) -> Result<(), PlatformVpnError> {
    let buffer_start = buffer_start as usize;
    let buffer_end = buffer_start
        .checked_add(buffer_bytes)
        .ok_or(PlatformVpnError::ProtocolViolation)?;
    let value_start = value as usize;
    let value_end = value_start
        .checked_add(value_bytes)
        .ok_or(PlatformVpnError::ProtocolViolation)?;
    if value.is_null() || value_start < buffer_start || value_end > buffer_end {
        Err(PlatformVpnError::ProtocolViolation)
    } else {
        Ok(())
    }
}

fn verify_authenticode_signer(artifact: &Path) -> Result<String, PlatformVpnError> {
    let mut path = wide(artifact.as_os_str());
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: path.as_mut_ptr(),
        hFile: ptr::null_mut(),
        pgKnownSubject: ptr::null_mut(),
    };
    let mut trust_data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: ptr::null_mut(),
        pSIPClientData: ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: ptr::null_mut(),
        pwszURLReference: ptr::null_mut(),
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT
            | WTD_CACHE_ONLY_URL_RETRIEVAL
            | WTD_DISABLE_MD2_MD4,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        pSignatureSettings: ptr::null_mut(),
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            ptr::null_mut(),
            &mut action,
            (&mut trust_data as *mut WINTRUST_DATA).cast::<c_void>(),
        )
    };
    let result = if status == 0 {
        signer_thumbprint_from_state(trust_data.hWVTStateData)
    } else {
        Err(PlatformVpnError::PermissionDenied)
    };
    trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(
            ptr::null_mut(),
            &mut action,
            (&mut trust_data as *mut WINTRUST_DATA).cast::<c_void>(),
        );
    }
    result
}

fn signer_thumbprint_from_state(state: HANDLE) -> Result<String, PlatformVpnError> {
    if state.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let provider = unsafe { WTHelperProvDataFromStateData(state) };
    if provider.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, 0, 0) };
    if signer.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let provider_cert = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if provider_cert.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let certificate = unsafe { (*provider_cert).pCert };
    if certificate.is_null() {
        return Err(PlatformVpnError::PermissionDenied);
    }
    let mut thumbprint = [0_u8; 20];
    let mut bytes = thumbprint.len() as u32;
    if unsafe {
        CertGetCertificateContextProperty(
            certificate,
            CERT_SHA1_HASH_PROP_ID,
            thumbprint.as_mut_ptr().cast(),
            &mut bytes,
        )
    } == 0
        || bytes as usize != thumbprint.len()
    {
        return Err(PlatformVpnError::PermissionDenied);
    }
    Ok(thumbprint
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect())
}

struct NativeLauncher;

impl SidecarLauncher for NativeLauncher {
    type Process = WindowsSidecarProcess;

    fn version_output(&self, artifact: &Path, cwd: &Path) -> Result<String, PlatformVpnError> {
        let mut command = fixed_command(artifact, cwd)?;
        command.arg("version");
        let output = run_bounded(command, HANDSHAKE_TIMEOUT)?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(PlatformVpnError::PermissionDenied);
        }
        String::from_utf8(output.stdout).map_err(|_| PlatformVpnError::ProtocolViolation)
    }

    fn check_config(
        &self,
        artifact: &Path,
        config: &Path,
        cwd: &Path,
    ) -> Result<(), PlatformVpnError> {
        let mut command = fixed_command(artifact, cwd)?;
        command.arg("check").arg("-c").arg(config);
        let output = run_bounded(command, HANDSHAKE_TIMEOUT)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(PlatformVpnError::InvalidConfiguration)
        }
    }

    fn spawn_run(
        &self,
        artifact: &Path,
        config: &Path,
        cwd: &Path,
        tun_probe: Arc<dyn TunStateProbe>,
    ) -> Result<Self::Process, PlatformVpnError> {
        let readiness = runtime_readiness(config)?;
        let mut command = fixed_command(artifact, cwd)?;
        command
            .arg("run")
            .arg("-c")
            .arg(config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let child = command.spawn().map_err(|_| PlatformVpnError::Unavailable)?;
        WindowsSidecarProcess::attach(child, tun_probe, readiness)
    }
}

fn fixed_command(artifact: &Path, cwd: &Path) -> Result<Command, PlatformVpnError> {
    let mut command = Command::new(artifact);
    command
        .current_dir(cwd)
        .env_clear()
        .env("SystemRoot", windows_directory()?)
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    Ok(command)
}

pub(crate) fn windows_directory() -> Result<OsString, PlatformVpnError> {
    let mut buffer = [0_u16; 32_768];
    let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(PlatformVpnError::Unavailable);
    }
    let directory = OsString::from_wide(&buffer[..length]);
    if !Path::new(&directory).is_absolute() {
        return Err(PlatformVpnError::ProtocolViolation);
    }
    Ok(directory)
}

struct BoundedOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_bounded(mut command: Command, timeout: Duration) -> Result<BoundedOutput, PlatformVpnError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| PlatformVpnError::Unavailable)?;
    let _job = match KillOnCloseJob::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            terminate_and_reap(&mut child);
            return Err(error);
        }
    };
    let stdout = child.stdout.take().ok_or(PlatformVpnError::Unavailable)?;
    let stderr = child.stderr.take().ok_or(PlatformVpnError::Unavailable)?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout));
    let stderr_reader = thread::spawn(move || read_bounded(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                terminate_and_reap(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(PlatformVpnError::Timeout);
            }
            Err(_) => {
                terminate_and_reap(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(PlatformVpnError::Unavailable);
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| PlatformVpnError::Unavailable)??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| PlatformVpnError::Unavailable)??;
    Ok(BoundedOutput {
        status,
        stdout,
        stderr,
    })
}

fn terminate_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_bounded(mut stream: impl Read) -> Result<Vec<u8>, PlatformVpnError> {
    let mut bytes = Vec::new();
    stream
        .by_ref()
        .take(MAX_HANDSHAKE_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PlatformVpnError::Unavailable)?;
    if bytes.len() as u64 > MAX_HANDSHAKE_OUTPUT_BYTES {
        return Err(PlatformVpnError::ProtocolViolation);
    }
    Ok(bytes)
}

struct KillOnCloseJob {
    handle: isize,
}

impl KillOnCloseJob {
    fn attach(child: &Child) -> Result<Self, PlatformVpnError> {
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(PlatformVpnError::Unavailable);
        }
        let job = Self {
            handle: handle as isize,
        };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast::<c_void>(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(job.raw(), child.as_raw_handle() as HANDLE) } == 0
        {
            return Err(PlatformVpnError::Unavailable);
        }
        Ok(job)
    }

    fn raw(&self) -> HANDLE {
        self.handle as HANDLE
    }
}

impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        if !self.raw().is_null() && self.raw() != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.raw());
            }
        }
    }
}

pub struct WindowsSidecarProcess {
    child: Child,
    _job: KillOnCloseJob,
    client: Arc<ManagedHostClient>,
    tun_probe: Arc<dyn TunStateProbe>,
    readiness: RuntimeReadiness,
    reaped: bool,
}

impl WindowsSidecarProcess {
    fn attach(
        mut child: Child,
        tun_probe: Arc<dyn TunStateProbe>,
        readiness: RuntimeReadiness,
    ) -> Result<Self, PlatformVpnError> {
        let job = match KillOnCloseJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PlatformVpnError::Unavailable);
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PlatformVpnError::Unavailable);
            }
        };
        let client = match ManagedHostClient::connect(stdin, stdout) {
            Ok(client) => client,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match error {
                    crate::managed_host::ClientError::ProtocolViolation => {
                        PlatformVpnError::ProtocolViolation
                    }
                    crate::managed_host::ClientError::TimedOut => PlatformVpnError::Timeout,
                    crate::managed_host::ClientError::Unavailable => PlatformVpnError::Unavailable,
                });
            }
        };
        Ok(Self {
            child,
            _job: job,
            client,
            tun_probe,
            readiness,
            reaped: false,
        })
    }

    fn client(&self) -> Arc<ManagedHostClient> {
        Arc::clone(&self.client)
    }

    fn reap(&mut self) -> Result<(), PlatformVpnError> {
        if self.reaped {
            return Ok(());
        }
        if self
            .child
            .try_wait()
            .map_err(|_| PlatformVpnError::Unavailable)?
            .is_none()
        {
            self.client.close();
            self.child
                .kill()
                .map_err(|_| PlatformVpnError::Unavailable)?;
        }
        self.child
            .wait()
            .map_err(|_| PlatformVpnError::Unavailable)?;
        self.reaped = true;
        Ok(())
    }
}

impl SupervisedDataPlaneProcess for WindowsSidecarProcess {
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
        match self.readiness {
            RuntimeReadiness::Tun => tun_readiness(self.tun_probe.as_ref()),
            RuntimeReadiness::MixedLoopback { port } => {
                mixed_listener_readiness(self.process_id(), port)
            }
        }
    }

    fn request_stop(&mut self) -> Result<(), PlatformVpnError> {
        self.client.close();
        Ok(())
    }

    fn force_stop(&mut self) -> Result<(), PlatformVpnError> {
        self.reap()
    }
}

impl Drop for WindowsSidecarProcess {
    fn drop(&mut self) {
        let _ = self.reap();
    }
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::TcpListener,
        process::Command,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use orange_platform::{DataPlaneSupervisorPolicy, PlatformVpnAdapter, SupervisedVpnAdapter};
    use tempfile::TempDir;

    use super::*;

    const TEST_SIGNER: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

    #[derive(Clone)]
    struct FixtureVerifier {
        signer: String,
        calls: Arc<AtomicUsize>,
    }

    impl SidecarTrustVerifier for FixtureVerifier {
        fn signer_sha1_thumbprint(&self, _artifact: &Path) -> Result<String, PlatformVpnError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.signer.clone())
        }
    }

    #[derive(Default)]
    struct FixtureTunProbe {
        state: Mutex<Option<TunInterfaceState>>,
        calls: AtomicUsize,
    }

    impl FixtureTunProbe {
        fn set(&self, state: Option<TunInterfaceState>) {
            *lock(&self.state) = state;
        }
    }

    impl TunStateProbe for FixtureTunProbe {
        fn orange_tun_state(&self) -> Result<Option<TunInterfaceState>, PlatformVpnError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(lock(&self.state).clone())
        }
    }

    struct FixtureState {
        checks: AtomicUsize,
        spawns: AtomicUsize,
        crashed: AtomicBool,
        stopped: AtomicBool,
        publish_tun_on_spawn: AtomicBool,
        tun_probe: Arc<FixtureTunProbe>,
    }

    impl Default for FixtureState {
        fn default() -> Self {
            Self {
                checks: AtomicUsize::new(0),
                spawns: AtomicUsize::new(0),
                crashed: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
                publish_tun_on_spawn: AtomicBool::new(true),
                tun_probe: Arc::new(FixtureTunProbe::default()),
            }
        }
    }

    #[derive(Clone)]
    struct FixtureLauncher {
        version: String,
        state: Arc<FixtureState>,
    }

    impl FixtureLauncher {
        fn tun_probe(&self) -> Arc<dyn TunStateProbe> {
            self.state.tun_probe.clone()
        }
    }

    impl SidecarLauncher for FixtureLauncher {
        type Process = FixtureProcess;

        fn version_output(
            &self,
            _artifact: &Path,
            _cwd: &Path,
        ) -> Result<String, PlatformVpnError> {
            Ok(self.version.clone())
        }

        fn check_config(
            &self,
            _artifact: &Path,
            _config: &Path,
            _cwd: &Path,
        ) -> Result<(), PlatformVpnError> {
            self.state.checks.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn spawn_run(
            &self,
            _artifact: &Path,
            _config: &Path,
            _cwd: &Path,
            tun_probe: Arc<dyn TunStateProbe>,
        ) -> Result<Self::Process, PlatformVpnError> {
            self.state.spawns.fetch_add(1, Ordering::Relaxed);
            if self.state.publish_tun_on_spawn.load(Ordering::Acquire) {
                self.state.tun_probe.set(Some(ready_tun_state()));
            }
            Ok(FixtureProcess {
                state: Arc::clone(&self.state),
                tun_probe,
            })
        }
    }

    struct FixtureProcess {
        state: Arc<FixtureState>,
        tun_probe: Arc<dyn TunStateProbe>,
    }

    impl SupervisedDataPlaneProcess for FixtureProcess {
        fn process_id(&self) -> u32 {
            71
        }

        fn try_wait(&mut self) -> Result<bool, PlatformVpnError> {
            Ok(self.state.crashed.load(Ordering::Acquire)
                || self.state.stopped.load(Ordering::Acquire))
        }

        fn readiness(&mut self) -> Result<ProcessReadiness, PlatformVpnError> {
            if self.try_wait()? {
                return Err(PlatformVpnError::Crashed);
            }
            tun_readiness(self.tun_probe.as_ref())
        }

        fn request_stop(&mut self) -> Result<(), PlatformVpnError> {
            self.state.stopped.store(true, Ordering::Release);
            self.state.tun_probe.set(None);
            Ok(())
        }

        fn force_stop(&mut self) -> Result<(), PlatformVpnError> {
            self.state.stopped.store(true, Ordering::Release);
            self.state.tun_probe.set(None);
            Ok(())
        }
    }

    fn tun_state(
        operational: bool,
        addresses: impl IntoIterator<Item = (IpAddr, u8)>,
    ) -> TunInterfaceState {
        TunInterfaceState {
            operational,
            unicast_addresses: addresses
                .into_iter()
                .map(|(address, prefix_length)| TunAddress {
                    address,
                    prefix_length,
                })
                .collect(),
        }
    }

    fn ready_tun_state() -> TunInterfaceState {
        tun_state(
            true,
            [
                (IpAddr::V4(TUN_IPV4_ADDRESS), TUN_IPV4_PREFIX_LENGTH),
                (IpAddr::V6(TUN_IPV6_ADDRESS), TUN_IPV6_PREFIX_LENGTH),
            ],
        )
    }

    fn manifest_for(artifact: &Path) -> RuntimeManifest {
        let mut manifest = RuntimeManifest::embedded().unwrap();
        manifest.artifact.sha256 = sha256_path(artifact, None).unwrap();
        manifest.artifact.allowed_signer_sha1_thumbprints = vec![TEST_SIGNER.to_owned()];
        manifest
    }

    fn version_output(manifest: &RuntimeManifest) -> String {
        format!(
            "sing-box version {}\r\n\r\nEnvironment: {} {}/{}\r\nTags: {}\r\nCGO: disabled\r\n",
            manifest.artifact.version,
            manifest.artifact.go_compiler,
            manifest.artifact.target.goos,
            manifest.artifact.target.goarch,
            manifest.artifact.build_tags.join(",")
        )
    }

    fn managed_host_fixture_process() -> Child {
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$payload=[Text.Encoding]::UTF8.GetBytes('{\"version\":1,\"kind\":\"ready\"}'); $stdout=[Console]::OpenStandardOutput(); $header=[byte[]](0,0,0,$payload.Length); $stdout.Write($header,0,4); $stdout.Write($payload,0,$payload.Length); $stdout.Flush(); [Console]::In.ReadToEnd() | Out-Null",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        command.spawn().unwrap()
    }

    fn fixture() -> (
        TempDir,
        RuntimeManifest,
        FixtureVerifier,
        FixtureLauncher,
        ConfigurationRevision,
    ) {
        let directory = TempDir::new().unwrap();
        let artifact = directory.path().join(FIXED_ARTIFACT_PATH);
        fs::write(&artifact, b"signed fixture").unwrap();
        let revision_root = directory.path().join(FIXED_REVISION_ROOT);
        fs::create_dir_all(&revision_root).unwrap();
        fs::write(revision_root.join("7.json"), b"{}").unwrap();
        let manifest = manifest_for(&artifact);
        let verifier = FixtureVerifier {
            signer: TEST_SIGNER.to_owned(),
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let launcher = FixtureLauncher {
            version: version_output(&manifest),
            state: Arc::new(FixtureState::default()),
        };
        (
            directory,
            manifest,
            verifier,
            launcher,
            ConfigurationRevision::new(7).unwrap(),
        )
    }

    #[test]
    fn embedded_manifest_is_strict_and_release_closed() {
        let manifest = RuntimeManifest::embedded().unwrap();
        assert!(!manifest.release_allowed);
        assert!(manifest.artifact.authenticode_required);
        assert!(manifest.artifact.allowed_signer_sha1_thumbprints.is_empty());
        assert_eq!(manifest.artifact.runtime_relative_path, FIXED_ARTIFACT_PATH);
        assert_eq!(manifest.revision_store.relative_path, FIXED_REVISION_ROOT);
    }

    #[test]
    fn manifest_rejects_unknown_fields_and_release_without_signer() {
        let mut document: serde_json::Value =
            serde_json::from_slice(RUNTIME_MANIFEST_BYTES).unwrap();
        document["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<RuntimeManifest>(document).is_err());

        let mut manifest = RuntimeManifest::embedded().unwrap();
        manifest.release_allowed = true;
        assert_eq!(
            manifest.validate(),
            Err(PlatformVpnError::InvalidConfiguration)
        );
    }

    #[test]
    fn version_handshake_is_exact() {
        let manifest = RuntimeManifest::embedded().unwrap();
        assert_eq!(
            manifest.verify_version_output(&version_output(&manifest)),
            Ok(())
        );
        assert_eq!(
            manifest.verify_version_output(&version_output(&manifest).replace(
                "Tags: with_quic,with_utls",
                "Tags: with_quic,with_utls,with_clash_api",
            )),
            Err(PlatformVpnError::PermissionDenied)
        );
    }

    #[test]
    fn tun_readiness_requires_up_interface_and_both_fixed_addresses() {
        let probe = FixtureTunProbe::default();
        assert_eq!(tun_readiness(&probe), Ok(ProcessReadiness::Pending));

        probe.set(Some(tun_state(
            true,
            [(IpAddr::V4(TUN_IPV4_ADDRESS), TUN_IPV4_PREFIX_LENGTH)],
        )));
        assert_eq!(tun_readiness(&probe), Ok(ProcessReadiness::Pending));

        probe.set(Some(tun_state(
            true,
            [
                (IpAddr::V4(Ipv4Addr::new(172, 19, 0, 2)), 30),
                (IpAddr::V6(TUN_IPV6_ADDRESS), TUN_IPV6_PREFIX_LENGTH),
            ],
        )));
        assert_eq!(tun_readiness(&probe), Ok(ProcessReadiness::Pending));

        let mut down = ready_tun_state();
        down.operational = false;
        probe.set(Some(down));
        assert_eq!(tun_readiness(&probe), Ok(ProcessReadiness::Pending));

        probe.set(Some(ready_tun_state()));
        assert_eq!(tun_readiness(&probe), Ok(ProcessReadiness::Ready));
    }

    #[test]
    fn runtime_readiness_is_mode_specific_and_fixed() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("config.json");
        for (document, expected) in [
            (
                format!(
                    r#"{{"inbounds":[{{"type":"mixed","listen":"127.0.0.1","listen_port":{SYSTEM_PROXY_LISTEN_PORT}}}]}}"#
                ),
                Ok(RuntimeReadiness::MixedLoopback {
                    port: SYSTEM_PROXY_LISTEN_PORT,
                }),
            ),
            (
                format!(
                    r#"{{"inbounds":[{{"type":"mixed","listen":"127.0.0.1","listen_port":{CANDIDATE_LISTEN_PORT}}}]}}"#
                ),
                Ok(RuntimeReadiness::MixedLoopback {
                    port: CANDIDATE_LISTEN_PORT,
                }),
            ),
            (
                r#"{"inbounds":[{"type":"tun","interface_name":"orange-tun"}]}"#.to_owned(),
                Ok(RuntimeReadiness::Tun),
            ),
        ] {
            fs::write(&path, document).unwrap();
            assert_eq!(runtime_readiness(&path), expected);
        }
        for rejected in [
            r#"{"inbounds":[{"type":"mixed","listen":"0.0.0.0","listen_port":24836}]}"#,
            r#"{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":24838}]}"#,
            r#"{"inbounds":[]}"#,
        ] {
            fs::write(&path, rejected).unwrap();
            assert_eq!(
                runtime_readiness(&path),
                Err(PlatformVpnError::InvalidConfiguration)
            );
        }
    }

    #[test]
    fn mixed_port_conflict_owned_by_another_process_fails_readiness() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(
            mixed_listener_readiness(std::process::id(), port),
            Ok(ProcessReadiness::Ready)
        );
        assert_eq!(
            mixed_listener_readiness(std::process::id().wrapping_add(1), port),
            Ok(ProcessReadiness::Pending)
        );
        assert_eq!(
            mixed_listener_readiness(std::process::id(), port.wrapping_add(1)),
            Ok(ProcessReadiness::Pending)
        );
    }

    #[test]
    fn preflight_rejects_stale_named_tun_before_trust_checks() {
        let (directory, manifest, verifier, launcher, revision) = fixture();
        let verifier_calls = Arc::clone(&verifier.calls);
        launcher
            .state
            .tun_probe
            .set(Some(tun_state(false, std::iter::empty())));
        let tun_probe = launcher.tun_probe();
        let backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();

        assert_eq!(
            backend.preflight_revision(revision),
            Err(PlatformVpnError::OperationInProgress)
        );
        assert_eq!(verifier_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn preflight_and_spawn_use_only_fixed_revision() {
        let (directory, manifest, verifier, launcher, revision) = fixture();
        let verifier_calls = Arc::clone(&verifier.calls);
        let state = Arc::clone(&launcher.state);
        let tun_probe = launcher.tun_probe();
        let backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();

        assert_eq!(backend.preflight_revision(revision), Ok(()));
        let mut process = backend.spawn_revision(revision).unwrap();
        assert_eq!(process.process_id(), 71);
        assert_eq!(process.readiness(), Ok(ProcessReadiness::Ready));
        assert_eq!(verifier_calls.load(Ordering::Relaxed), 1);
        assert_eq!(state.checks.load(Ordering::Relaxed), 1);
        assert_eq!(state.spawns.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn empty_or_wrong_signer_allowlist_fails_closed() {
        let (directory, mut manifest, verifier, launcher, revision) = fixture();
        manifest.artifact.allowed_signer_sha1_thumbprints.clear();
        let tun_probe = launcher.tun_probe();
        let backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();
        assert_eq!(
            backend.preflight_revision(revision),
            Err(PlatformVpnError::PermissionDenied)
        );
    }

    #[test]
    fn changed_config_between_preflight_and_spawn_is_rejected() {
        let (directory, manifest, verifier, launcher, revision) = fixture();
        let tun_probe = launcher.tun_probe();
        let backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();
        backend.preflight_revision(revision).unwrap();
        fs::write(
            directory.path().join(FIXED_REVISION_ROOT).join("7.json"),
            b"{\"changed\":true}",
        )
        .unwrap();
        assert!(matches!(
            backend.spawn_revision(revision),
            Err(PlatformVpnError::PermissionDenied)
        ));
    }

    #[test]
    fn spawn_rejects_tun_that_appears_after_preflight() {
        let (directory, manifest, verifier, launcher, revision) = fixture();
        let state = Arc::clone(&launcher.state);
        let tun_probe = launcher.tun_probe();
        let backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();
        backend.preflight_revision(revision).unwrap();
        state.tun_probe.set(Some(ready_tun_state()));

        assert!(matches!(
            backend.spawn_revision(revision),
            Err(PlatformVpnError::OperationInProgress)
        ));
        assert_eq!(state.spawns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn revision_symlink_escape_is_rejected() {
        use std::os::windows::fs::symlink_file;

        let (directory, manifest, verifier, launcher, revision) = fixture();
        let config = directory.path().join(FIXED_REVISION_ROOT).join("7.json");
        fs::remove_file(&config).unwrap();
        let outside = directory.path().join("outside.json");
        fs::write(&outside, b"{}").unwrap();
        symlink_file(&outside, &config).unwrap();
        let tun_probe = launcher.tun_probe();
        let backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();
        assert_eq!(
            backend.preflight_revision(revision),
            Err(PlatformVpnError::PermissionDenied)
        );
    }

    #[test]
    fn supervisor_detects_fixture_process_crash() {
        let (directory, manifest, verifier, launcher, revision) = fixture();
        let state = Arc::clone(&launcher.state);
        let tun_probe = launcher.tun_probe();
        let backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();
        let adapter = SupervisedVpnAdapter::new(
            backend,
            DataPlaneSupervisorPolicy::new(
                Duration::from_millis(10),
                Duration::from_secs(1),
                Duration::from_millis(50),
            )
            .unwrap(),
        )
        .unwrap();
        adapter.start(revision).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while adapter.snapshot().unwrap().state() != orange_domain::DataPlaneState::Online {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        state.tun_probe.set(None);
        state.crashed.store(true, Ordering::Release);
        while adapter.snapshot().unwrap().state() != orange_domain::DataPlaneState::Failed {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!adapter.snapshot().unwrap().has_active_instance());
    }

    #[test]
    fn cleanup_waits_until_owned_tun_disappears() {
        let (directory, manifest, verifier, launcher, _revision) = fixture();
        let state = Arc::clone(&launcher.state);
        let tun_probe = launcher.tun_probe();
        let mut backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();
        backend.cleanup_policy = TunCleanupPolicy {
            timeout: Duration::from_millis(200),
            poll_interval: Duration::from_millis(5),
        };
        state.tun_probe.set(Some(ready_tun_state()));
        let delayed_probe = Arc::clone(&state.tun_probe);
        let removal = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            delayed_probe.set(None);
        });

        assert_eq!(backend.cleanup_tun(), Ok(()));
        removal.join().unwrap();
    }

    #[test]
    fn cleanup_fails_when_owned_tun_remains() {
        let (directory, manifest, verifier, launcher, _revision) = fixture();
        launcher.state.tun_probe.set(Some(ready_tun_state()));
        let tun_probe = launcher.tun_probe();
        let mut backend =
            BackendCore::new(directory.path(), manifest, verifier, launcher, tun_probe).unwrap();
        backend.cleanup_policy = TunCleanupPolicy {
            timeout: Duration::from_millis(20),
            poll_interval: Duration::from_millis(5),
        };

        assert_eq!(backend.cleanup_tun(), Err(PlatformVpnError::CleanupFailed));
    }

    #[test]
    fn native_process_force_stop_reaps_child() {
        let child = managed_host_fixture_process();
        let tun_probe = Arc::new(FixtureTunProbe::default());
        let mut process =
            WindowsSidecarProcess::attach(child, tun_probe, RuntimeReadiness::Tun).unwrap();
        assert!(!process.try_wait().unwrap());
        process.force_stop().unwrap();
        assert!(process.try_wait().unwrap());
    }

    #[test]
    fn native_process_closes_control_stdin_for_graceful_stop() {
        let child = managed_host_fixture_process();
        let tun_probe = Arc::new(FixtureTunProbe::default());
        let mut process =
            WindowsSidecarProcess::attach(child, tun_probe, RuntimeReadiness::Tun).unwrap();
        process.request_stop().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !process.try_wait().unwrap() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn native_win_verify_trust_rejects_unsigned_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"not an authenticode image").unwrap();
        assert_eq!(
            verify_authenticode_signer(file.path()),
            Err(PlatformVpnError::PermissionDenied)
        );
    }

    #[test]
    fn native_tun_probe_reads_windows_adapter_table() {
        let _ = query_orange_tun_state().unwrap();
    }

    #[test]
    fn native_windows_directory_is_absolute() {
        assert!(Path::new(&windows_directory().unwrap()).is_absolute());
    }

    #[test]
    fn bounded_handshake_timeout_reaps_child() {
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .creation_flags(CREATE_NO_WINDOW);
        let started = Instant::now();
        assert!(matches!(
            run_bounded(command, Duration::from_millis(50)),
            Err(PlatformVpnError::Timeout)
        ));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
