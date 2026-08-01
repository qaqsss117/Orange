use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{Arc, RwLock},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const RULE_RESOURCE_MANIFEST_SCHEMA_VERSION: u16 = 1;
const PINNED_RULE_SING_BOX_VERSION: &str = "1.13.14";
const PINNED_SRS_VERSION: u16 = 2;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_RESOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RESOURCE_COUNT: usize = 64;
const MAX_ID_BYTES: usize = 64;
const MAX_NAME_BYTES: usize = 96;
const MAX_SOURCE_BYTES: usize = 256;
const MAX_LICENSE_BYTES: usize = 128;
const MMDB_METADATA_MARKER: &[u8] = b"\xAB\xCD\xEFMaxMind.com";
const WINDOWS_REPARSE_POINT_ATTRIBUTE: u32 = 0x0400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleResourceFormat {
    Srs,
    Mmdb,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleResourceId(String);

impl RuleResourceId {
    pub fn new(value: impl Into<String>) -> Result<Self, RuleResourceError> {
        let value = value.into();
        if !valid_identifier(&value) {
            return Err(RuleResourceError::InvalidResourceId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleResourceError {
    InvalidRoot,
    InvalidManifest,
    ManifestTooLarge,
    UnsupportedSchema,
    UnsupportedSingBoxVersion,
    InvalidResourceId,
    DuplicateResource,
    UnsafePath,
    MissingResource,
    PermissionDenied,
    SizeMismatch,
    HashMismatch,
    FormatMismatch,
    NoActiveManifest,
    UnknownResource,
    StateUnavailable,
}

pub trait SharedRuleResourceRootVerifier: Send + Sync {
    fn verify_shared_rule_root(&self, canonical_root: &Path) -> Result<(), RuleResourceError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleResourceActivation {
    manifest_id: String,
    resource_count: usize,
}

impl RuleResourceActivation {
    pub fn manifest_id(&self) -> &str {
        &self.manifest_id
    }

    pub const fn resource_count(&self) -> usize {
        self.resource_count
    }
}

#[derive(Clone)]
pub struct RuleResourceStore {
    root: PathBuf,
    shared_root_verifier: Option<Arc<dyn SharedRuleResourceRootVerifier>>,
    active: Arc<RwLock<Option<ValidatedManifest>>>,
}

impl RuleResourceStore {
    pub fn open_user_private(root: impl AsRef<Path>) -> Result<Self, RuleResourceError> {
        Self::open(root.as_ref(), None)
    }

    pub fn open_service_shared(
        root: impl AsRef<Path>,
        verifier: Arc<dyn SharedRuleResourceRootVerifier>,
    ) -> Result<Self, RuleResourceError> {
        Self::open(root.as_ref(), Some(verifier))
    }

    fn open(
        root: &Path,
        verifier: Option<Arc<dyn SharedRuleResourceRootVerifier>>,
    ) -> Result<Self, RuleResourceError> {
        let root = canonical_private_root(root)?;
        if let Some(verifier) = &verifier {
            verifier.verify_shared_rule_root(&root)?;
            validate_shared_permissions(&root)?;
        }
        Ok(Self {
            root,
            shared_root_verifier: verifier,
            active: Arc::new(RwLock::new(None)),
        })
    }

    pub fn activate_manifest(
        &self,
        document: &[u8],
    ) -> Result<RuleResourceActivation, RuleResourceError> {
        self.validate_root()?;
        if document.is_empty() || document.len() > MAX_MANIFEST_BYTES {
            return Err(if document.len() > MAX_MANIFEST_BYTES {
                RuleResourceError::ManifestTooLarge
            } else {
                RuleResourceError::InvalidManifest
            });
        }
        let manifest: RuleResourceManifest =
            serde_json::from_slice(document).map_err(|_| RuleResourceError::InvalidManifest)?;
        let candidate = self.validate_manifest(manifest)?;
        let activation = RuleResourceActivation {
            manifest_id: candidate.manifest_id.clone(),
            resource_count: candidate.resources.len(),
        };
        *self
            .active
            .write()
            .map_err(|_| RuleResourceError::StateUnavailable)? = Some(candidate);
        Ok(activation)
    }

    pub fn active_manifest_id(&self) -> Result<Option<String>, RuleResourceError> {
        Ok(self
            .active
            .read()
            .map_err(|_| RuleResourceError::StateUnavailable)?
            .as_ref()
            .map(|active| active.manifest_id.clone()))
    }

    pub fn resolve(&self, id: &RuleResourceId) -> Result<PathBuf, RuleResourceError> {
        self.validate_root()?;
        let entry = self
            .active
            .read()
            .map_err(|_| RuleResourceError::StateUnavailable)?
            .as_ref()
            .ok_or(RuleResourceError::NoActiveManifest)?
            .resources
            .get(id)
            .cloned()
            .ok_or(RuleResourceError::UnknownResource)?;
        self.validate_resource(&entry)
    }

    fn validate_root(&self) -> Result<(), RuleResourceError> {
        let current = canonical_private_root(&self.root)?;
        if !same_path(&current, &self.root) {
            return Err(RuleResourceError::UnsafePath);
        }
        if let Some(verifier) = &self.shared_root_verifier {
            verifier.verify_shared_rule_root(&current)?;
            validate_shared_permissions(&current)?;
        }
        Ok(())
    }

    fn validate_manifest(
        &self,
        manifest: RuleResourceManifest,
    ) -> Result<ValidatedManifest, RuleResourceError> {
        if manifest.schema_version != RULE_RESOURCE_MANIFEST_SCHEMA_VERSION {
            return Err(RuleResourceError::UnsupportedSchema);
        }
        if !valid_identifier(&manifest.manifest_id)
            || manifest.resources.is_empty()
            || manifest.resources.len() > MAX_RESOURCE_COUNT
        {
            return Err(RuleResourceError::InvalidManifest);
        }

        let mut resources = BTreeMap::new();
        let mut normalized_names = BTreeSet::new();
        for entry in manifest.resources {
            let id = RuleResourceId::new(entry.id.clone())?;
            validate_entry(&entry)?;
            if !normalized_names.insert(entry.name.to_ascii_lowercase())
                || resources.contains_key(&id)
            {
                return Err(RuleResourceError::DuplicateResource);
            }
            self.validate_resource(&entry)?;
            resources.insert(id, entry);
        }
        Ok(ValidatedManifest {
            manifest_id: manifest.manifest_id,
            resources,
        })
    }

    fn validate_resource(&self, entry: &RuleResourceEntry) -> Result<PathBuf, RuleResourceError> {
        let path = self.root.join(&entry.name);
        let path_metadata = fs::symlink_metadata(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => RuleResourceError::MissingResource,
            std::io::ErrorKind::PermissionDenied => RuleResourceError::PermissionDenied,
            _ => RuleResourceError::UnsafePath,
        })?;
        if !path_metadata.is_file() || metadata_is_link_or_reparse(&path_metadata) {
            return Err(RuleResourceError::UnsafePath);
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| RuleResourceError::UnsafePath)?;
        if canonical
            .parent()
            .is_none_or(|parent| !same_path(parent, &self.root))
        {
            return Err(RuleResourceError::UnsafePath);
        }

        let file = File::open(&canonical).map_err(|error| match error.kind() {
            std::io::ErrorKind::PermissionDenied => RuleResourceError::PermissionDenied,
            _ => RuleResourceError::MissingResource,
        })?;
        let metadata = file
            .metadata()
            .map_err(|_| RuleResourceError::PermissionDenied)?;
        if metadata.len() != entry.size_bytes || metadata.len() > MAX_RESOURCE_BYTES {
            return Err(RuleResourceError::SizeMismatch);
        }
        validate_non_executable_permissions(&metadata)?;
        if self.shared_root_verifier.is_some() {
            validate_shared_permissions_metadata(&metadata)?;
        }
        let capacity =
            usize::try_from(metadata.len()).map_err(|_| RuleResourceError::SizeMismatch)?;
        let mut content = Vec::with_capacity(capacity);
        file.take(MAX_RESOURCE_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|_| RuleResourceError::PermissionDenied)?;
        if content.len() as u64 != entry.size_bytes {
            return Err(RuleResourceError::SizeMismatch);
        }
        let digest = format!("{:x}", Sha256::digest(&content));
        if digest != entry.sha256 {
            return Err(RuleResourceError::HashMismatch);
        }
        validate_format(entry, &content)?;
        Ok(canonical)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleResourceManifest {
    schema_version: u16,
    manifest_id: String,
    resources: Vec<RuleResourceEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleResourceEntry {
    id: String,
    name: String,
    format: RuleResourceFormat,
    format_version: u16,
    sing_box_version: String,
    sha256: String,
    size_bytes: u64,
    source: RuleResourceSource,
    license: String,
    generated_at: String,
    expires_at: String,
    signature: RuleResourceSignature,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleResourceSource {
    repository: String,
    commit: String,
    output_commit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleResourceSignature {
    status: String,
    algorithm: String,
    key_id: String,
    value: String,
}

#[derive(Debug)]
struct ValidatedManifest {
    manifest_id: String,
    resources: BTreeMap<RuleResourceId, RuleResourceEntry>,
}

fn validate_entry(entry: &RuleResourceEntry) -> Result<(), RuleResourceError> {
    if !valid_resource_name(&entry.name, entry.format)
        || entry.sing_box_version != PINNED_RULE_SING_BOX_VERSION
        || entry.size_bytes == 0
        || entry.size_bytes > MAX_RESOURCE_BYTES
        || !valid_lower_hex(&entry.sha256, 64)
        || !valid_repository(&entry.source.repository)
        || !valid_lower_hex(&entry.source.commit, 40)
        || !valid_lower_hex(&entry.source.output_commit, 40)
        || !valid_bounded_text(&entry.license, MAX_LICENSE_BYTES)
        || !valid_utc_timestamp(&entry.generated_at)
        || !valid_utc_timestamp(&entry.expires_at)
        || entry.generated_at >= entry.expires_at
        || !valid_signature(&entry.signature)
    {
        return Err(RuleResourceError::InvalidManifest);
    }
    match entry.format {
        RuleResourceFormat::Srs if entry.format_version == PINNED_SRS_VERSION => Ok(()),
        RuleResourceFormat::Mmdb if entry.format_version > 0 => Ok(()),
        _ => Err(RuleResourceError::FormatMismatch),
    }
}

fn validate_format(entry: &RuleResourceEntry, content: &[u8]) -> Result<(), RuleResourceError> {
    let valid = match entry.format {
        RuleResourceFormat::Srs => {
            content.get(..4) == Some([b'S', b'R', b'S', entry.format_version as u8].as_slice())
        }
        RuleResourceFormat::Mmdb => content
            .windows(MMDB_METADATA_MARKER.len())
            .rev()
            .take(128 * 1024)
            .any(|window| window == MMDB_METADATA_MARKER),
    };
    if valid {
        Ok(())
    } else {
        Err(RuleResourceError::FormatMismatch)
    }
}

fn canonical_private_root(root: &Path) -> Result<PathBuf, RuleResourceError> {
    if !root.is_absolute() {
        return Err(RuleResourceError::InvalidRoot);
    }
    let metadata = fs::symlink_metadata(root).map_err(|_| RuleResourceError::InvalidRoot)?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(RuleResourceError::UnsafePath);
    }
    root.canonicalize()
        .map_err(|_| RuleResourceError::InvalidRoot)
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && !value.contains("--")
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'0'..=b'9' => true,
            b'-' => index > 0 && index + 1 < value.len(),
            _ => false,
        })
}

fn valid_resource_name(value: &str, format: RuleResourceFormat) -> bool {
    if value.is_empty()
        || value.len() > MAX_NAME_BYTES
        || value != value.to_ascii_lowercase()
        || Path::new(value).components().count() != 1
        || !matches!(
            Path::new(value).components().next(),
            Some(Component::Normal(_))
        )
        || value.contains(['/', '\\', ':'])
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'!' | b'-')
        })
    {
        return false;
    }
    match format {
        RuleResourceFormat::Srs => value.ends_with(".srs"),
        RuleResourceFormat::Mmdb => value.ends_with(".mmdb"),
    }
}

fn valid_repository(value: &str) -> bool {
    if !valid_bounded_text(value, MAX_SOURCE_BYTES) {
        return false;
    }
    let mut parts = value.split('/');
    let valid_part = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    };
    matches!((parts.next(), parts.next(), parts.next()), (Some(owner), Some(repo), None) if valid_part(owner) && valid_part(repo))
}

fn valid_bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'<' | b'>' | b'"' | b'\''))
}

fn valid_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_utc_timestamp(value: &str) -> bool {
    if value.len() != 20
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || value.as_bytes()[10] != b'T'
        || value.as_bytes()[13] != b':'
        || value.as_bytes()[16] != b':'
        || value.as_bytes()[19] != b'Z'
        || value.bytes().enumerate().any(|(index, byte)| {
            !matches!(index, 4 | 7 | 10 | 13 | 16 | 19) && !byte.is_ascii_digit()
        })
    {
        return false;
    }
    let number = |start: usize, end: usize| value[start..end].parse::<u32>().ok();
    let Some(year @ 2000..=9999) = number(0, 4) else {
        return false;
    };
    let Some(month @ 1..=12) = number(5, 7) else {
        return false;
    };
    let Some(day) = number(8, 10) else {
        return false;
    };
    let days_in_month = match month {
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day >= 1
        && day <= days_in_month
        && matches!(number(11, 13), Some(0..=23))
        && matches!(number(14, 16), Some(0..=59))
        && matches!(number(17, 19), Some(0..=59))
}

fn valid_signature(signature: &RuleResourceSignature) -> bool {
    match signature.status.as_str() {
        "unsigned-development-bundle" => {
            signature.algorithm == "none" && signature.key_id == "none" && signature.value == "none"
        }
        "verified-release-signature" => {
            signature.algorithm == "ed25519"
                && valid_identifier(&signature.key_id)
                && valid_bounded_text(&signature.value, 256)
                && signature.value != "none"
        }
        _ => false,
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn metadata_is_link_or_reparse(metadata: &Metadata) -> bool {
    unsafe_path_flags(
        metadata.file_type().is_symlink(),
        windows_file_attributes(metadata),
    )
}

fn unsafe_path_flags(is_symlink: bool, windows_attributes: u32) -> bool {
    is_symlink || windows_attributes & WINDOWS_REPARSE_POINT_ATTRIBUTE != 0
}

#[cfg(windows)]
fn windows_file_attributes(metadata: &Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes()
}

#[cfg(not(windows))]
fn windows_file_attributes(_metadata: &Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn validate_non_executable_permissions(metadata: &Metadata) -> Result<(), RuleResourceError> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o111 == 0 {
        Ok(())
    } else {
        Err(RuleResourceError::PermissionDenied)
    }
}

#[cfg(not(unix))]
fn validate_non_executable_permissions(_metadata: &Metadata) -> Result<(), RuleResourceError> {
    Ok(())
}

#[cfg(unix)]
fn validate_shared_permissions(path: &Path) -> Result<(), RuleResourceError> {
    let metadata = fs::metadata(path).map_err(|_| RuleResourceError::PermissionDenied)?;
    validate_shared_permissions_metadata(&metadata)
}

#[cfg(not(unix))]
fn validate_shared_permissions(_path: &Path) -> Result<(), RuleResourceError> {
    Ok(())
}

#[cfg(unix)]
fn validate_shared_permissions_metadata(metadata: &Metadata) -> Result<(), RuleResourceError> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o022 == 0 {
        Ok(())
    } else {
        Err(RuleResourceError::PermissionDenied)
    }
}

#[cfg(not(unix))]
fn validate_shared_permissions_metadata(_metadata: &Metadata) -> Result<(), RuleResourceError> {
    Ok(())
}
