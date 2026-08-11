use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, atomic::AtomicU64},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use orange_domain::{ConnectionMode, NodeSelectionMode, RoutingMode};

use crate::vpn::ConfigurationRevision;

pub const SETTINGS_SCHEMA_VERSION: u16 = 6;
const STORAGE_FORMAT_VERSION: u16 = 1;
const STORE_DIRECTORY: &str = "state-v1";
const FILE_PREFIX: &str = "settings-";
const FILE_SUFFIX: &str = ".json";
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024;
const MAX_PERSISTED_SELECTORS: usize = 8;
const MAX_PERSISTED_ID_BYTES: usize = 64;

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn new_installation_id() -> Result<String, PersistenceError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| PersistenceError::EntropyUnavailable)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut id = String::with_capacity(32);
    for byte in bytes {
        id.push(char::from(HEX[usize::from(byte >> 4)]));
        id.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalePreference {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en-US")]
    EnUs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReducedMotionPreference {
    System,
    Reduce,
    Allow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceUpdateOutcome {
    Changed,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataPlaneRevisionLedger {
    current_revision: Option<u64>,
    previous_revision: Option<u64>,
    candidate_revision: Option<u64>,
}

impl DataPlaneRevisionLedger {
    pub fn current_revision(&self) -> Option<ConfigurationRevision> {
        self.current_revision.map(valid_revision)
    }

    pub fn previous_revision(&self) -> Option<ConfigurationRevision> {
        self.previous_revision.map(valid_revision)
    }

    pub fn candidate_revision(&self) -> Option<ConfigurationRevision> {
        self.candidate_revision.map(valid_revision)
    }

    pub fn stage_candidate(
        &mut self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        let revision = revision.get();
        if self.candidate_revision == Some(revision)
            || (self.current_revision == Some(revision) && self.candidate_revision.is_none())
        {
            return Ok(PersistenceUpdateOutcome::Unchanged);
        }
        if self.candidate_revision.is_some() {
            return Err(PersistenceError::InvalidState);
        }
        self.candidate_revision = Some(revision);
        Ok(PersistenceUpdateOutcome::Changed)
    }

    pub fn commit_candidate_online(
        &mut self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        let revision = revision.get();
        if self.current_revision == Some(revision) && self.candidate_revision.is_none() {
            return Ok(PersistenceUpdateOutcome::Unchanged);
        }
        if self.candidate_revision != Some(revision) {
            return Err(PersistenceError::InvalidState);
        }
        self.previous_revision = self.current_revision;
        self.current_revision = Some(revision);
        self.candidate_revision = None;
        Ok(PersistenceUpdateOutcome::Changed)
    }

    pub fn reject_candidate(
        &mut self,
        revision: ConfigurationRevision,
    ) -> Result<Option<ConfigurationRevision>, PersistenceError> {
        if self.candidate_revision != Some(revision.get()) {
            return Err(PersistenceError::InvalidState);
        }
        self.candidate_revision = None;
        Ok(self.current_revision())
    }

    pub fn active_failure_target(&self) -> Option<ConfigurationRevision> {
        self.previous_revision()
    }

    pub fn commit_rollback(
        &mut self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        let revision = revision.get();
        if self.current_revision == Some(revision) && self.candidate_revision.is_none() {
            return Ok(PersistenceUpdateOutcome::Unchanged);
        }
        if self.previous_revision != Some(revision) {
            return Err(PersistenceError::InvalidState);
        }
        std::mem::swap(&mut self.current_revision, &mut self.previous_revision);
        self.candidate_revision = None;
        Ok(PersistenceUpdateOutcome::Changed)
    }

    fn validate(&self) -> Result<(), PersistenceError> {
        let revisions = [
            self.current_revision,
            self.previous_revision,
            self.candidate_revision,
        ];
        if revisions
            .into_iter()
            .flatten()
            .any(|revision| revision == 0)
            || self.current_revision.is_some() && self.current_revision == self.previous_revision
            || self.current_revision.is_some() && self.current_revision == self.candidate_revision
        {
            return Err(PersistenceError::InvalidSettings);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataPlaneNodeSelectionLedger {
    revision: Option<u64>,
    selected_nodes: BTreeMap<String, String>,
}

impl DataPlaneNodeSelectionLedger {
    pub fn new(
        revision: ConfigurationRevision,
        selections: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, PersistenceError> {
        let mut selected_nodes = BTreeMap::new();
        for (selector_id, node_id) in selections {
            if selected_nodes.insert(selector_id, node_id).is_some() {
                return Err(PersistenceError::InvalidSettings);
            }
        }
        let ledger = Self {
            revision: Some(revision.get()),
            selected_nodes,
        };
        ledger.validate()?;
        Ok(ledger)
    }

    pub fn revision(&self) -> Option<ConfigurationRevision> {
        self.revision.map(valid_revision)
    }

    pub fn selected_node(&self, selector_id: &str) -> Option<&str> {
        self.selected_nodes.get(selector_id).map(String::as_str)
    }

    pub fn selections(&self) -> impl Iterator<Item = (&str, &str)> {
        self.selected_nodes
            .iter()
            .map(|(selector_id, node_id)| (selector_id.as_str(), node_id.as_str()))
    }

    fn validate(&self) -> Result<(), PersistenceError> {
        if self.selected_nodes.len() > MAX_PERSISTED_SELECTORS
            || self.revision.is_none() && !self.selected_nodes.is_empty()
            || self.revision.is_some() && self.selected_nodes.is_empty()
            || self.revision == Some(0)
            || self.selected_nodes.iter().any(|(selector_id, node_id)| {
                !valid_persisted_id(selector_id) || !valid_persisted_id(node_id)
            })
        {
            return Err(PersistenceError::InvalidSettings);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSettings {
    schema_version: u16,
    locale: LocalePreference,
    launch_on_startup: bool,
    connection_mode: ConnectionMode,
    routing_mode: RoutingMode,
    theme: ThemePreference,
    reduced_motion: ReducedMotionPreference,
    data_plane: DataPlaneRevisionLedger,
    node_selection: DataPlaneNodeSelectionLedger,
    node_selection_mode: NodeSelectionMode,
    installation_id: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self::new().expect("secure installation identifier entropy must be available")
    }
}

impl AppSettings {
    fn new() -> Result<Self, PersistenceError> {
        Ok(Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            locale: LocalePreference::System,
            launch_on_startup: false,
            connection_mode: ConnectionMode::SystemProxy,
            routing_mode: RoutingMode::Smart,
            theme: ThemePreference::System,
            reduced_motion: ReducedMotionPreference::System,
            data_plane: DataPlaneRevisionLedger::default(),
            node_selection: DataPlaneNodeSelectionLedger::default(),
            node_selection_mode: NodeSelectionMode::Auto,
            installation_id: new_installation_id()?,
        })
    }

    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn locale(&self) -> LocalePreference {
        self.locale
    }

    pub const fn launch_on_startup(&self) -> bool {
        self.launch_on_startup
    }

    pub const fn connection_mode(&self) -> ConnectionMode {
        self.connection_mode
    }

    pub const fn routing_mode(&self) -> RoutingMode {
        self.routing_mode
    }

    pub const fn theme(&self) -> ThemePreference {
        self.theme
    }

    pub const fn reduced_motion(&self) -> ReducedMotionPreference {
        self.reduced_motion
    }

    pub const fn data_plane(&self) -> &DataPlaneRevisionLedger {
        &self.data_plane
    }

    pub const fn data_plane_mut(&mut self) -> &mut DataPlaneRevisionLedger {
        &mut self.data_plane
    }

    pub const fn node_selection(&self) -> &DataPlaneNodeSelectionLedger {
        &self.node_selection
    }

    pub const fn node_selection_mode(&self) -> NodeSelectionMode {
        self.node_selection_mode
    }

    pub fn installation_id(&self) -> &str {
        &self.installation_id
    }

    pub fn set_locale(&mut self, locale: LocalePreference) {
        self.locale = locale;
    }

    pub fn set_launch_on_startup(&mut self, enabled: bool) {
        self.launch_on_startup = enabled;
    }

    pub fn set_connection_mode(&mut self, mode: ConnectionMode) {
        self.connection_mode = mode;
    }

    pub fn set_routing_mode(&mut self, mode: RoutingMode) {
        self.routing_mode = mode;
    }

    pub fn set_theme(&mut self, theme: ThemePreference) {
        self.theme = theme;
    }

    pub fn set_reduced_motion(&mut self, reduced_motion: ReducedMotionPreference) {
        self.reduced_motion = reduced_motion;
    }

    pub fn set_node_selection_mode(&mut self, mode: NodeSelectionMode) {
        self.node_selection_mode = mode;
    }

    fn validate(&self) -> Result<(), PersistenceError> {
        if self.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(PersistenceError::InvalidSettings);
        }
        self.data_plane.validate()?;
        self.node_selection.validate()?;
        if self.installation_id.len() != 32
            || !self
                .installation_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(PersistenceError::InvalidSettings);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSettings {
    settings: AppSettings,
    generation: u64,
    migrated_from_schema: Option<u64>,
    recovered_from_generation: Option<u64>,
}

impl LoadedSettings {
    pub const fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn into_settings(self) -> AppSettings {
        self.settings
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn migrated_from_schema(&self) -> Option<u64> {
        self.migrated_from_schema
    }

    pub const fn recovered_from_generation(&self) -> Option<u64> {
        self.recovered_from_generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceError {
    InvalidStoragePath,
    InvalidSettings,
    InvalidState,
    CorruptData,
    DocumentTooLarge,
    UnsupportedStorageVersion { found: u64, supported: u16 },
    UnsupportedSchemaVersion { found: u64, supported: u16 },
    GenerationOverflow,
    EntropyUnavailable,
    Io,
}

impl PersistenceError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidStoragePath => "persistence-invalid-storage-path",
            Self::InvalidSettings => "persistence-invalid-settings",
            Self::InvalidState => "persistence-invalid-state",
            Self::CorruptData => "persistence-corrupt-data",
            Self::DocumentTooLarge => "persistence-document-too-large",
            Self::UnsupportedStorageVersion { .. } => "persistence-storage-version-unsupported",
            Self::UnsupportedSchemaVersion { .. } => "persistence-schema-version-unsupported",
            Self::GenerationOverflow => "persistence-generation-overflow",
            Self::EntropyUnavailable => "persistence-entropy-unavailable",
            Self::Io => "persistence-io-failure",
        }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for PersistenceError {}

pub trait SettingsStorage: Send + Sync {
    fn load(&self) -> Result<LoadedSettings, PersistenceError>;
    fn save(&self, settings: &AppSettings) -> Result<u64, PersistenceError>;
}

pub trait DataPlaneRevisionStorage: Send + Sync {
    fn load_revision_ledger(&self) -> Result<DataPlaneRevisionLedger, PersistenceError>;

    fn stage_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError>;

    fn commit_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError>;

    fn reject_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<Option<ConfigurationRevision>, PersistenceError>;

    fn commit_revision_rollback(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError>;
}

impl<S> DataPlaneRevisionStorage for Arc<S>
where
    S: DataPlaneRevisionStorage + ?Sized,
{
    fn load_revision_ledger(&self) -> Result<DataPlaneRevisionLedger, PersistenceError> {
        (**self).load_revision_ledger()
    }

    fn stage_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        (**self).stage_revision_candidate(revision)
    }

    fn commit_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        (**self).commit_revision_candidate(revision)
    }

    fn reject_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<Option<ConfigurationRevision>, PersistenceError> {
        (**self).reject_revision_candidate(revision)
    }

    fn commit_revision_rollback(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        (**self).commit_revision_rollback(revision)
    }
}

pub trait DataPlaneNodeSelectionStorage: Send + Sync {
    fn load_node_selections(&self) -> Result<DataPlaneNodeSelectionLedger, PersistenceError>;

    fn replace_node_selections(
        &self,
        ledger: &DataPlaneNodeSelectionLedger,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError>;
}

impl<S> DataPlaneNodeSelectionStorage for Arc<S>
where
    S: DataPlaneNodeSelectionStorage + ?Sized,
{
    fn load_node_selections(&self) -> Result<DataPlaneNodeSelectionLedger, PersistenceError> {
        (**self).load_node_selections()
    }

    fn replace_node_selections(
        &self,
        ledger: &DataPlaneNodeSelectionLedger,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        (**self).replace_node_selections(ledger)
    }
}

pub struct FileSettingsStore {
    directory: PathBuf,
    write_lock: Mutex<()>,
}

impl FileSettingsStore {
    pub fn new(app_data_directory: impl Into<PathBuf>) -> Result<Self, PersistenceError> {
        let app_data_directory = app_data_directory.into();
        if !app_data_directory.is_absolute() {
            return Err(PersistenceError::InvalidStoragePath);
        }
        Ok(Self {
            directory: app_data_directory.join(STORE_DIRECTORY),
            write_lock: Mutex::new(()),
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn load_node_selection_preferences(
        &self,
    ) -> Result<(NodeSelectionMode, String), PersistenceError> {
        let _guard = lock(&self.write_lock);
        let settings = self.load_locked()?;
        Ok((
            settings.settings().node_selection_mode(),
            settings.settings().installation_id().to_owned(),
        ))
    }

    pub fn replace_node_selection_mode(
        &self,
        mode: NodeSelectionMode,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        let _guard = lock(&self.write_lock);
        let mut settings = self.load_locked()?.into_settings();
        if settings.node_selection_mode() == mode {
            return Ok(PersistenceUpdateOutcome::Unchanged);
        }
        settings.set_node_selection_mode(mode);
        self.save_locked(&settings)?;
        Ok(PersistenceUpdateOutcome::Changed)
    }

    fn load_locked(&self) -> Result<LoadedSettings, PersistenceError> {
        let candidates = self.candidate_files()?;
        if candidates.is_empty() {
            return Ok(LoadedSettings {
                settings: AppSettings::new()?,
                generation: 0,
                migrated_from_schema: None,
                recovered_from_generation: None,
            });
        }

        let mut recoverable_error = None;
        for (index, candidate) in candidates.iter().enumerate() {
            match self.read_document(candidate) {
                Ok(parsed) => {
                    let source_generation = candidate.generation;
                    let recovered_from_generation = (index > 0).then_some(source_generation);
                    let migrated_from_schema = parsed.migrated_from_schema;
                    let settings = parsed.settings;
                    if recovered_from_generation.is_some() || migrated_from_schema.is_some() {
                        let generation = self.save_locked(&settings)?;
                        return Ok(LoadedSettings {
                            settings,
                            generation,
                            migrated_from_schema,
                            recovered_from_generation,
                        });
                    }
                    return Ok(LoadedSettings {
                        settings,
                        generation: source_generation,
                        migrated_from_schema: None,
                        recovered_from_generation: None,
                    });
                }
                Err(
                    error @ (PersistenceError::CorruptData | PersistenceError::DocumentTooLarge),
                ) => {
                    recoverable_error.get_or_insert(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(recoverable_error.unwrap_or(PersistenceError::CorruptData))
    }

    fn save_locked(&self, settings: &AppSettings) -> Result<u64, PersistenceError> {
        settings.validate()?;
        let candidates = self.candidate_files()?;
        let (generation, previous_valid_generation) = self.prepare_save(&candidates)?;
        self.commit(settings, generation)?;
        self.prune(&candidates, generation, previous_valid_generation);
        Ok(generation)
    }

    fn update_revision_ledger<R>(
        &self,
        update: impl FnOnce(&mut DataPlaneRevisionLedger) -> Result<R, PersistenceError>,
    ) -> Result<R, PersistenceError> {
        let _guard = lock(&self.write_lock);
        let mut settings = self.load_locked()?.into_settings();
        let before = settings.data_plane().clone();
        let result = update(settings.data_plane_mut())?;
        if settings.data_plane() != &before {
            self.save_locked(&settings)?;
        }
        Ok(result)
    }

    fn prepare_save(
        &self,
        candidates: &[CandidateFile],
    ) -> Result<(u64, Option<u64>), PersistenceError> {
        let maximum_generation = candidates
            .first()
            .map_or(0, |candidate| candidate.generation);
        let generation = maximum_generation
            .checked_add(1)
            .ok_or(PersistenceError::GenerationOverflow)?;
        let mut saw_recoverable_error = false;
        for candidate in candidates {
            match self.read_document(candidate) {
                Ok(_) => return Ok((generation, Some(candidate.generation))),
                Err(PersistenceError::CorruptData | PersistenceError::DocumentTooLarge) => {
                    saw_recoverable_error = true;
                }
                Err(error) => return Err(error),
            }
        }
        if saw_recoverable_error {
            Err(PersistenceError::CorruptData)
        } else {
            Ok((generation, None))
        }
    }

    fn commit(&self, settings: &AppSettings, generation: u64) -> Result<(), PersistenceError> {
        let document = CurrentStorageDocument {
            storage_version: STORAGE_FORMAT_VERSION,
            generation,
            settings,
        };
        let mut bytes = serde_json::to_vec_pretty(&document).map_err(|_| PersistenceError::Io)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(PersistenceError::DocumentTooLarge);
        }

        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary_path = self.directory.join(format!(
            ".settings-{generation:020}-{}-{sequence:020}.tmp",
            std::process::id()
        ));
        let final_path = self.directory.join(file_name(generation));
        if final_path.exists() {
            return Err(PersistenceError::CorruptData);
        }

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut temporary = TemporaryFile::new(temporary_path.clone());
        let mut file = options
            .open(&temporary_path)
            .map_err(|_| PersistenceError::Io)?;
        file.write_all(&bytes).map_err(|_| PersistenceError::Io)?;
        file.sync_all().map_err(|_| PersistenceError::Io)?;
        drop(file);

        fs::rename(&temporary_path, &final_path).map_err(|_| PersistenceError::Io)?;
        temporary.committed = true;
        sync_directory(&self.directory)?;
        Ok(())
    }

    fn prune(
        &self,
        existing: &[CandidateFile],
        committed_generation: u64,
        previous_valid_generation: Option<u64>,
    ) {
        let retained = [Some(committed_generation), previous_valid_generation];
        for candidate in existing {
            if !retained.contains(&Some(candidate.generation)) {
                let _ = fs::remove_file(&candidate.path);
            }
        }
    }

    fn read_document(&self, candidate: &CandidateFile) -> Result<ParsedSettings, PersistenceError> {
        let metadata = fs::symlink_metadata(&candidate.path).map_err(|_| PersistenceError::Io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PersistenceError::InvalidStoragePath);
        }
        if metadata.len() > MAX_DOCUMENT_BYTES {
            return Err(PersistenceError::DocumentTooLarge);
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(&candidate.path)
            .map_err(|_| PersistenceError::Io)?
            .take(MAX_DOCUMENT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| PersistenceError::Io)?;
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(PersistenceError::DocumentTooLarge);
        }
        let document: StoredDocument =
            serde_json::from_slice(&bytes).map_err(|_| PersistenceError::CorruptData)?;
        if document.storage_version != u64::from(STORAGE_FORMAT_VERSION) {
            if document.storage_version > u64::from(STORAGE_FORMAT_VERSION) {
                return Err(PersistenceError::UnsupportedStorageVersion {
                    found: document.storage_version,
                    supported: STORAGE_FORMAT_VERSION,
                });
            }
            return Err(PersistenceError::CorruptData);
        }
        if document.generation != candidate.generation {
            return Err(PersistenceError::CorruptData);
        }
        parse_settings(document.settings)
    }

    fn candidate_files(&self) -> Result<Vec<CandidateFile>, PersistenceError> {
        self.ensure_directory()?;
        let mut candidates = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(|_| PersistenceError::Io)? {
            let entry = entry.map_err(|_| PersistenceError::Io)?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if let Some(generation) = parse_generation(&name) {
                candidates.push(CandidateFile {
                    generation,
                    path: entry.path(),
                });
            }
        }
        candidates.sort_unstable_by_key(|candidate| std::cmp::Reverse(candidate.generation));
        Ok(candidates)
    }

    fn ensure_directory(&self) -> Result<(), PersistenceError> {
        if let Some(parent) = self.directory.parent() {
            fs::create_dir_all(parent).map_err(|_| PersistenceError::Io)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true).mode(0o700);
            builder
                .create(&self.directory)
                .map_err(|_| PersistenceError::Io)?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(&self.directory).map_err(|_| PersistenceError::Io)?;

        let metadata = fs::symlink_metadata(&self.directory).map_err(|_| PersistenceError::Io)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(PersistenceError::InvalidStoragePath);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| PersistenceError::Io)?;
        }
        Ok(())
    }
}

impl SettingsStorage for FileSettingsStore {
    fn load(&self) -> Result<LoadedSettings, PersistenceError> {
        let _guard = lock(&self.write_lock);
        self.load_locked()
    }

    fn save(&self, settings: &AppSettings) -> Result<u64, PersistenceError> {
        let _guard = lock(&self.write_lock);
        self.save_locked(settings)
    }
}

impl DataPlaneRevisionStorage for FileSettingsStore {
    fn load_revision_ledger(&self) -> Result<DataPlaneRevisionLedger, PersistenceError> {
        let _guard = lock(&self.write_lock);
        Ok(self.load_locked()?.settings().data_plane().clone())
    }

    fn stage_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        self.update_revision_ledger(|ledger| ledger.stage_candidate(revision))
    }

    fn commit_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        self.update_revision_ledger(|ledger| ledger.commit_candidate_online(revision))
    }

    fn reject_revision_candidate(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<Option<ConfigurationRevision>, PersistenceError> {
        self.update_revision_ledger(|ledger| ledger.reject_candidate(revision))
    }

    fn commit_revision_rollback(
        &self,
        revision: ConfigurationRevision,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        self.update_revision_ledger(|ledger| ledger.commit_rollback(revision))
    }
}

impl DataPlaneNodeSelectionStorage for FileSettingsStore {
    fn load_node_selections(&self) -> Result<DataPlaneNodeSelectionLedger, PersistenceError> {
        let _guard = lock(&self.write_lock);
        Ok(self.load_locked()?.settings().node_selection().clone())
    }

    fn replace_node_selections(
        &self,
        ledger: &DataPlaneNodeSelectionLedger,
    ) -> Result<PersistenceUpdateOutcome, PersistenceError> {
        ledger.validate()?;
        let _guard = lock(&self.write_lock);
        let mut settings = self.load_locked()?.into_settings();
        if settings.node_selection() == ledger {
            return Ok(PersistenceUpdateOutcome::Unchanged);
        }
        settings.node_selection = ledger.clone();
        self.save_locked(&settings)?;
        Ok(PersistenceUpdateOutcome::Changed)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentStorageDocument<'a> {
    storage_version: u16,
    generation: u64,
    settings: &'a AppSettings,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDocument {
    storage_version: u64,
    generation: u64,
    settings: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSchemaProbe {
    schema_version: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppSettingsV1 {
    schema_version: u16,
    locale: LocalePreference,
    launch_on_startup: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppSettingsV2 {
    schema_version: u16,
    locale: LocalePreference,
    launch_on_startup: bool,
    theme: ThemePreference,
    reduced_motion: ReducedMotionPreference,
    data_plane: DataPlaneRevisionLedger,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppSettingsV3 {
    schema_version: u16,
    locale: LocalePreference,
    launch_on_startup: bool,
    theme: ThemePreference,
    reduced_motion: ReducedMotionPreference,
    data_plane: DataPlaneRevisionLedger,
    #[serde(rename = "nodeSelection")]
    _node_selection: DataPlaneNodeSelectionLedger,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppSettingsV4 {
    schema_version: u16,
    locale: LocalePreference,
    launch_on_startup: bool,
    connection_mode: ConnectionMode,
    theme: ThemePreference,
    reduced_motion: ReducedMotionPreference,
    data_plane: DataPlaneRevisionLedger,
    #[serde(rename = "nodeSelection")]
    _node_selection: DataPlaneNodeSelectionLedger,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppSettingsV5 {
    schema_version: u16,
    locale: LocalePreference,
    launch_on_startup: bool,
    connection_mode: ConnectionMode,
    routing_mode: RoutingMode,
    theme: ThemePreference,
    reduced_motion: ReducedMotionPreference,
    data_plane: DataPlaneRevisionLedger,
    #[serde(rename = "nodeSelection")]
    _node_selection: DataPlaneNodeSelectionLedger,
}

struct ParsedSettings {
    settings: AppSettings,
    migrated_from_schema: Option<u64>,
}

struct CandidateFile {
    generation: u64,
    path: PathBuf,
}

struct TemporaryFile {
    path: PathBuf,
    committed: bool,
}

impl TemporaryFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn parse_settings(value: Value) -> Result<ParsedSettings, PersistenceError> {
    let probe: SettingsSchemaProbe =
        serde_json::from_value(value.clone()).map_err(|_| PersistenceError::CorruptData)?;
    match probe.schema_version {
        1 => {
            let legacy: AppSettingsV1 =
                serde_json::from_value(value).map_err(|_| PersistenceError::CorruptData)?;
            if legacy.schema_version != 1 {
                return Err(PersistenceError::CorruptData);
            }
            let settings = AppSettings {
                schema_version: SETTINGS_SCHEMA_VERSION,
                locale: legacy.locale,
                launch_on_startup: legacy.launch_on_startup,
                connection_mode: ConnectionMode::SystemProxy,
                routing_mode: RoutingMode::Smart,
                theme: ThemePreference::System,
                reduced_motion: ReducedMotionPreference::System,
                data_plane: DataPlaneRevisionLedger::default(),
                node_selection: DataPlaneNodeSelectionLedger::default(),
                node_selection_mode: NodeSelectionMode::Auto,
                installation_id: new_installation_id()?,
            };
            settings.validate()?;
            Ok(ParsedSettings {
                settings,
                migrated_from_schema: Some(1),
            })
        }
        2 => {
            let legacy: AppSettingsV2 =
                serde_json::from_value(value).map_err(|_| PersistenceError::CorruptData)?;
            if legacy.schema_version != 2 {
                return Err(PersistenceError::CorruptData);
            }
            let settings = AppSettings {
                schema_version: SETTINGS_SCHEMA_VERSION,
                locale: legacy.locale,
                launch_on_startup: legacy.launch_on_startup,
                connection_mode: ConnectionMode::SystemProxy,
                routing_mode: RoutingMode::Smart,
                theme: legacy.theme,
                reduced_motion: legacy.reduced_motion,
                data_plane: legacy.data_plane,
                node_selection: DataPlaneNodeSelectionLedger::default(),
                node_selection_mode: NodeSelectionMode::Auto,
                installation_id: new_installation_id()?,
            };
            settings.validate()?;
            Ok(ParsedSettings {
                settings,
                migrated_from_schema: Some(2),
            })
        }
        3 => {
            let legacy: AppSettingsV3 =
                serde_json::from_value(value).map_err(|_| PersistenceError::CorruptData)?;
            if legacy.schema_version != 3 {
                return Err(PersistenceError::CorruptData);
            }
            let settings = AppSettings {
                schema_version: SETTINGS_SCHEMA_VERSION,
                locale: legacy.locale,
                launch_on_startup: legacy.launch_on_startup,
                connection_mode: ConnectionMode::SystemProxy,
                routing_mode: RoutingMode::Smart,
                theme: legacy.theme,
                reduced_motion: legacy.reduced_motion,
                data_plane: legacy.data_plane,
                node_selection: DataPlaneNodeSelectionLedger::default(),
                node_selection_mode: NodeSelectionMode::Auto,
                installation_id: new_installation_id()?,
            };
            settings.validate()?;
            Ok(ParsedSettings {
                settings,
                migrated_from_schema: Some(3),
            })
        }
        4 => {
            let legacy: AppSettingsV4 =
                serde_json::from_value(value).map_err(|_| PersistenceError::CorruptData)?;
            if legacy.schema_version != 4 {
                return Err(PersistenceError::CorruptData);
            }
            let settings = AppSettings {
                schema_version: SETTINGS_SCHEMA_VERSION,
                locale: legacy.locale,
                launch_on_startup: legacy.launch_on_startup,
                connection_mode: legacy.connection_mode,
                routing_mode: RoutingMode::Smart,
                theme: legacy.theme,
                reduced_motion: legacy.reduced_motion,
                data_plane: legacy.data_plane,
                node_selection: DataPlaneNodeSelectionLedger::default(),
                node_selection_mode: NodeSelectionMode::Auto,
                installation_id: new_installation_id()?,
            };
            settings.validate()?;
            Ok(ParsedSettings {
                settings,
                migrated_from_schema: Some(4),
            })
        }
        5 => {
            let legacy: AppSettingsV5 =
                serde_json::from_value(value).map_err(|_| PersistenceError::CorruptData)?;
            if legacy.schema_version != 5 {
                return Err(PersistenceError::CorruptData);
            }
            let settings = AppSettings {
                schema_version: SETTINGS_SCHEMA_VERSION,
                locale: legacy.locale,
                launch_on_startup: legacy.launch_on_startup,
                connection_mode: legacy.connection_mode,
                routing_mode: legacy.routing_mode,
                theme: legacy.theme,
                reduced_motion: legacy.reduced_motion,
                data_plane: legacy.data_plane,
                node_selection: DataPlaneNodeSelectionLedger::default(),
                node_selection_mode: NodeSelectionMode::Auto,
                installation_id: new_installation_id()?,
            };
            settings.validate()?;
            Ok(ParsedSettings {
                settings,
                migrated_from_schema: Some(5),
            })
        }
        version if version == u64::from(SETTINGS_SCHEMA_VERSION) => {
            let settings: AppSettings =
                serde_json::from_value(value).map_err(|_| PersistenceError::CorruptData)?;
            settings.validate()?;
            Ok(ParsedSettings {
                settings,
                migrated_from_schema: None,
            })
        }
        version if version > u64::from(SETTINGS_SCHEMA_VERSION) => {
            Err(PersistenceError::UnsupportedSchemaVersion {
                found: version,
                supported: SETTINGS_SCHEMA_VERSION,
            })
        }
        _ => Err(PersistenceError::CorruptData),
    }
}

fn file_name(generation: u64) -> String {
    format!("{FILE_PREFIX}{generation:020}{FILE_SUFFIX}")
}

fn parse_generation(name: &str) -> Option<u64> {
    let digits = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    (digits.len() == 20 && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
        .filter(|generation| *generation > 0)
}

fn valid_revision(value: u64) -> ConfigurationRevision {
    ConfigurationRevision::new(value).expect("validated persisted revision must be non-zero")
}

fn valid_persisted_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PERSISTED_ID_BYTES
        && !value.starts_with("orange-")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn sync_directory(directory: &Path) -> Result<(), PersistenceError> {
    #[cfg(unix)]
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| PersistenceError::Io)?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with_selection() -> (AppSettings, DataPlaneNodeSelectionLedger) {
        let ledger = DataPlaneNodeSelectionLedger::new(
            ConfigurationRevision::new(7).expect("non-zero revision"),
            [("proxy".to_owned(), "node-02".to_owned())],
        )
        .expect("valid selection ledger");
        let mut settings = AppSettings::default();
        settings.node_selection = ledger.clone();
        settings.node_selection_mode = NodeSelectionMode::Manual;
        (settings, ledger)
    }

    #[test]
    fn schema_five_migration_resets_selection_and_enables_auto_mode() {
        let (settings, _) = settings_with_selection();
        let mut value = serde_json::to_value(settings).expect("settings serialize");
        let object = value.as_object_mut().expect("settings object");
        object.insert("schemaVersion".to_owned(), Value::from(5));
        object.remove("nodeSelectionMode");
        object.remove("installationId");

        let parsed = parse_settings(value).expect("schema five migrates");
        assert_eq!(parsed.migrated_from_schema, Some(5));
        assert_eq!(
            parsed.settings.node_selection,
            DataPlaneNodeSelectionLedger::default()
        );
        assert_eq!(parsed.settings.node_selection_mode, NodeSelectionMode::Auto);
        assert_eq!(parsed.settings.installation_id.len(), 32);
        assert!(
            parsed
                .settings
                .installation_id
                .bytes()
                .all(|byte| { byte.is_ascii_digit() || matches!(byte, b'a'..=b'f') })
        );
    }

    #[test]
    fn current_schema_retains_selection_preferences() {
        let (settings, ledger) = settings_with_selection();
        let installation_id = settings.installation_id.clone();
        let value = serde_json::to_value(settings).expect("settings serialize");

        let parsed = parse_settings(value).expect("current settings parse");
        assert_eq!(parsed.migrated_from_schema, None);
        assert_eq!(parsed.settings.node_selection, ledger);
        assert_eq!(
            parsed.settings.node_selection_mode,
            NodeSelectionMode::Manual
        );
        assert_eq!(parsed.settings.installation_id, installation_id);
    }
}
