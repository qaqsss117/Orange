use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, atomic::AtomicU64},
};

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::vpn::ConfigurationRevision;

pub const SETTINGS_SCHEMA_VERSION: u16 = 3;
const STORAGE_FORMAT_VERSION: u16 = 1;
const STORE_DIRECTORY: &str = "state-v1";
const FILE_PREFIX: &str = "settings-";
const FILE_SUFFIX: &str = ".json";
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024;
const MAX_PERSISTED_SELECTORS: usize = 8;
const MAX_PERSISTED_ID_BYTES: usize = 64;

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    theme: ThemePreference,
    reduced_motion: ReducedMotionPreference,
    data_plane: DataPlaneRevisionLedger,
    node_selection: DataPlaneNodeSelectionLedger,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            locale: LocalePreference::System,
            launch_on_startup: false,
            theme: ThemePreference::System,
            reduced_motion: ReducedMotionPreference::System,
            data_plane: DataPlaneRevisionLedger::default(),
            node_selection: DataPlaneNodeSelectionLedger::default(),
        }
    }
}

impl AppSettings {
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    pub const fn locale(&self) -> LocalePreference {
        self.locale
    }

    pub const fn launch_on_startup(&self) -> bool {
        self.launch_on_startup
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

    pub fn set_locale(&mut self, locale: LocalePreference) {
        self.locale = locale;
    }

    pub fn set_launch_on_startup(&mut self, enabled: bool) {
        self.launch_on_startup = enabled;
    }

    pub fn set_theme(&mut self, theme: ThemePreference) {
        self.theme = theme;
    }

    pub fn set_reduced_motion(&mut self, reduced_motion: ReducedMotionPreference) {
        self.reduced_motion = reduced_motion;
    }

    fn validate(&self) -> Result<(), PersistenceError> {
        if self.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(PersistenceError::InvalidSettings);
        }
        self.data_plane.validate()?;
        self.node_selection.validate()
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
    #[cfg(test)]
    fail_next_commit: AtomicBool,
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
            #[cfg(test)]
            fail_next_commit: AtomicBool::new(false),
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    #[cfg(test)]
    fn fail_next_commit(&self) {
        self.fail_next_commit.store(true, Ordering::SeqCst);
    }

    fn load_locked(&self) -> Result<LoadedSettings, PersistenceError> {
        let candidates = self.candidate_files()?;
        if candidates.is_empty() {
            return Ok(LoadedSettings {
                settings: AppSettings::default(),
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

        #[cfg(test)]
        if self.fail_next_commit.swap(false, Ordering::SeqCst) {
            return Err(PersistenceError::Io);
        }

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
                theme: ThemePreference::System,
                reduced_motion: ReducedMotionPreference::System,
                data_plane: DataPlaneRevisionLedger::default(),
                node_selection: DataPlaneNodeSelectionLedger::default(),
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
                theme: legacy.theme,
                reduced_motion: legacy.reduced_motion,
                data_plane: legacy.data_plane,
                node_selection: DataPlaneNodeSelectionLedger::default(),
            };
            settings.validate()?;
            Ok(ParsedSettings {
                settings,
                migrated_from_schema: Some(2),
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
    use std::{collections::HashMap, sync::Mutex};

    use tempfile::TempDir;

    use crate::{SecretKey, SecretStorage, SecretStoreBackend, SecretStoreError, SecretValue};

    use super::*;

    const SETTINGS_V1: &str = include_str!("../../../contracts/settings/fixtures/settings.v1.json");
    const SETTINGS_V2: &str = include_str!("../../../contracts/settings/fixtures/settings.v2.json");
    const SETTINGS_V3: &str = include_str!("../../../contracts/settings/fixtures/settings.v3.json");
    const SCHEMA_V1: &str = include_str!("../../../contracts/settings/settings.schema.v1.json");
    const SCHEMA_V2: &str = include_str!("../../../contracts/settings/settings.schema.v2.json");
    const SCHEMA_V3: &str = include_str!("../../../contracts/settings/settings.schema.v3.json");

    #[derive(Default)]
    struct MemorySecrets {
        values: Mutex<HashMap<SecretKey, Vec<u8>>>,
    }

    impl SecretStoreBackend for MemorySecrets {
        fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError> {
            lock(&self.values).insert(key, value.to_vec());
            Ok(())
        }

        fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
            lock(&self.values)
                .get(&key)
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
            lock(&self.values).remove(&key);
            Ok(())
        }
    }

    #[test]
    fn settings_fixtures_and_schemas_migrate_exactly() {
        let expected: AppSettings = serde_json::from_str(SETTINGS_V3).unwrap();
        for (fixture, version) in [(SETTINGS_V1, 1), (SETTINGS_V2, 2)] {
            let migrated = parse_settings(serde_json::from_str(fixture).unwrap()).unwrap();
            assert_eq!(migrated.settings, expected);
            assert_eq!(migrated.migrated_from_schema, Some(version));
        }

        let schema_v1: Value = serde_json::from_str(SCHEMA_V1).unwrap();
        let schema_v2: Value = serde_json::from_str(SCHEMA_V2).unwrap();
        let schema_v3: Value = serde_json::from_str(SCHEMA_V3).unwrap();
        assert_eq!(schema_v1["properties"]["schemaVersion"]["const"], 1);
        assert_eq!(schema_v2["properties"]["schemaVersion"]["const"], 2);
        assert_eq!(
            schema_v3["properties"]["schemaVersion"]["const"],
            SETTINGS_SCHEMA_VERSION
        );
        assert_eq!(schema_v3["additionalProperties"], false);
    }

    #[test]
    fn revision_ledger_selects_candidate_and_active_failure_rollbacks() {
        let mut ledger = DataPlaneRevisionLedger::default();
        let first = ConfigurationRevision::new(1).unwrap();
        let second = ConfigurationRevision::new(2).unwrap();

        assert_eq!(
            ledger.stage_candidate(first),
            Ok(PersistenceUpdateOutcome::Changed)
        );
        assert_eq!(
            ledger.commit_candidate_online(first),
            Ok(PersistenceUpdateOutcome::Changed)
        );
        assert_eq!(ledger.current_revision(), Some(first));
        assert_eq!(ledger.previous_revision(), None);

        ledger.stage_candidate(second).unwrap();
        assert_eq!(ledger.reject_candidate(second), Ok(Some(first)));
        ledger.stage_candidate(second).unwrap();
        ledger.commit_candidate_online(second).unwrap();
        assert_eq!(ledger.current_revision(), Some(second));
        assert_eq!(ledger.active_failure_target(), Some(first));
        ledger.commit_rollback(first).unwrap();
        assert_eq!(ledger.current_revision(), Some(first));
        assert_eq!(ledger.previous_revision(), Some(second));
    }

    #[test]
    fn revision_storage_transactions_preserve_preferences_and_are_durable() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        let mut settings = AppSettings::default();
        settings.set_locale(LocalePreference::ZhCn);
        settings.set_theme(ThemePreference::Dark);
        store.save(&settings).unwrap();
        let revision = ConfigurationRevision::new(9).unwrap();

        assert_eq!(
            store.stage_revision_candidate(revision),
            Ok(PersistenceUpdateOutcome::Changed)
        );
        assert_eq!(
            store.load_revision_ledger().unwrap().candidate_revision(),
            Some(revision)
        );
        assert_eq!(
            store.commit_revision_candidate(revision),
            Ok(PersistenceUpdateOutcome::Changed)
        );

        let reopened = FileSettingsStore::new(root.path()).unwrap();
        let loaded = reopened.load().unwrap();
        assert_eq!(loaded.settings().locale(), LocalePreference::ZhCn);
        assert_eq!(loaded.settings().theme(), ThemePreference::Dark);
        assert_eq!(
            loaded.settings().data_plane().current_revision(),
            Some(revision)
        );
        assert_eq!(loaded.settings().data_plane().candidate_revision(), None);
    }

    #[test]
    fn failed_revision_commit_preserves_the_candidate_marker_for_recovery() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        let revision = ConfigurationRevision::new(12).unwrap();
        store.stage_revision_candidate(revision).unwrap();

        store.fail_next_commit();
        assert_eq!(
            store.commit_revision_candidate(revision),
            Err(PersistenceError::Io)
        );
        let ledger = store.load_revision_ledger().unwrap();
        assert_eq!(ledger.current_revision(), None);
        assert_eq!(ledger.candidate_revision(), Some(revision));

        assert_eq!(
            store.commit_revision_candidate(revision),
            Ok(PersistenceUpdateOutcome::Changed)
        );
        assert_eq!(
            store.load_revision_ledger().unwrap().current_revision(),
            Some(revision)
        );
    }

    #[test]
    fn node_selection_storage_is_bounded_atomic_and_durable() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        let mut settings = AppSettings::default();
        settings.set_theme(ThemePreference::Dark);
        store.save(&settings).unwrap();
        let revision = ConfigurationRevision::new(17).unwrap();
        let ledger = DataPlaneNodeSelectionLedger::new(
            revision,
            [("proxy".to_owned(), "node-sg".to_owned())],
        )
        .unwrap();

        assert_eq!(
            store.replace_node_selections(&ledger),
            Ok(PersistenceUpdateOutcome::Changed)
        );
        assert_eq!(
            store.replace_node_selections(&ledger),
            Ok(PersistenceUpdateOutcome::Unchanged)
        );
        let reopened = FileSettingsStore::new(root.path()).unwrap();
        let loaded = reopened.load().unwrap();
        assert_eq!(loaded.settings().theme(), ThemePreference::Dark);
        assert_eq!(loaded.settings().node_selection(), &ledger);
        assert_eq!(
            reopened
                .load_node_selections()
                .unwrap()
                .selected_node("proxy"),
            Some("node-sg")
        );

        let mut invalid = serde_json::to_value(&ledger).unwrap();
        invalid["selectedNodes"] = serde_json::json!({"orange-internal": "node-sg"});
        assert!(
            serde_json::from_value::<DataPlaneNodeSelectionLedger>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn atomic_commit_failure_preserves_the_previous_generation() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        let mut first = AppSettings::default();
        first.set_locale(LocalePreference::ZhCn);
        assert_eq!(store.save(&first), Ok(1));

        let mut replacement = first.clone();
        replacement.set_locale(LocalePreference::EnUs);
        store.fail_next_commit();
        assert_eq!(store.save(&replacement), Err(PersistenceError::Io));
        fs::write(
            store.directory().join(".settings-killed-before-rename.tmp"),
            b"{truncated",
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.settings(), &first);
        assert_eq!(loaded.generation(), 1);
        assert_eq!(committed_files(&store).len(), 1);
    }

    #[test]
    fn persisted_revision_ledger_survives_reopen() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        let mut settings = AppSettings::default();
        let first = ConfigurationRevision::new(41).unwrap();
        let second = ConfigurationRevision::new(42).unwrap();
        settings.data_plane_mut().stage_candidate(first).unwrap();
        settings
            .data_plane_mut()
            .commit_candidate_online(first)
            .unwrap();
        settings.data_plane_mut().stage_candidate(second).unwrap();
        settings
            .data_plane_mut()
            .commit_candidate_online(second)
            .unwrap();
        store.save(&settings).unwrap();

        let reopened = FileSettingsStore::new(root.path()).unwrap();
        let loaded = reopened.load().unwrap();
        assert_eq!(
            loaded.settings().data_plane().current_revision(),
            Some(second)
        );
        assert_eq!(
            loaded.settings().data_plane().previous_revision(),
            Some(first)
        );
        assert_eq!(
            loaded.settings().data_plane().active_failure_target(),
            Some(first)
        );
    }

    #[test]
    #[cfg(unix)]
    fn unix_store_permissions_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        store.save(&AppSettings::default()).unwrap();
        let directory_mode = fs::metadata(store.directory())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let file_mode = fs::metadata(store.directory().join(file_name(1)))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }

    #[test]
    fn corrupt_latest_generation_recovers_and_promotes_the_previous_one() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        let mut first = AppSettings::default();
        first.set_theme(ThemePreference::Dark);
        store.save(&first).unwrap();
        let mut second = first.clone();
        second.set_theme(ThemePreference::Light);
        store.save(&second).unwrap();
        fs::write(store.directory().join(file_name(2)), b"{truncated").unwrap();

        let recovered = store.load().unwrap();
        assert_eq!(recovered.settings(), &first);
        assert_eq!(recovered.generation(), 3);
        assert_eq!(recovered.recovered_from_generation(), Some(1));
        assert_eq!(committed_files(&store), vec![1, 3]);
    }

    #[test]
    fn migration_commit_failure_leaves_the_v1_fixture_intact() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        write_document(&store, 7, serde_json::from_str(SETTINGS_V1).unwrap());
        let legacy_path = store.directory().join(file_name(7));
        let original = fs::read(&legacy_path).unwrap();

        store.fail_next_commit();
        assert_eq!(store.load(), Err(PersistenceError::Io));
        assert_eq!(fs::read(&legacy_path).unwrap(), original);
        assert_eq!(committed_files(&store), vec![7]);

        let reopened = FileSettingsStore::new(root.path()).unwrap();
        let migrated = reopened.load().unwrap();
        assert_eq!(migrated.generation(), 8);
        assert_eq!(migrated.migrated_from_schema(), Some(1));
        assert_eq!(
            migrated.settings(),
            &serde_json::from_str::<AppSettings>(SETTINGS_V3).unwrap()
        );
    }

    #[test]
    fn future_schema_blocks_load_and_save_without_discarding_data() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        let future = serde_json::json!({
            "schemaVersion": 99,
            "locale": "system"
        });
        write_document(&store, 1, future);

        let expected = PersistenceError::UnsupportedSchemaVersion {
            found: 99,
            supported: SETTINGS_SCHEMA_VERSION,
        };
        assert_eq!(store.load(), Err(expected));
        assert_eq!(store.save(&AppSettings::default()), Err(expected));
        assert_eq!(committed_files(&store), vec![1]);
    }

    #[test]
    fn logout_removes_every_user_secret_but_retains_settings() {
        let root = TempDir::new().unwrap();
        let store = FileSettingsStore::new(root.path()).unwrap();
        let mut settings = AppSettings::default();
        settings.set_launch_on_startup(true);
        store.save(&settings).unwrap();

        let secrets = SecretStorage::new(MemorySecrets::default());
        for key in [
            SecretKey::AccessToken,
            SecretKey::RefreshToken,
            SecretKey::SubscriptionCredential,
        ] {
            let mut value = SecretValue::new(b"unusable-test-credential".to_vec()).unwrap();
            secrets.store(key, &mut value).unwrap();
        }
        secrets.logout().unwrap();
        for key in [
            SecretKey::AccessToken,
            SecretKey::RefreshToken,
            SecretKey::SubscriptionCredential,
        ] {
            assert!(secrets.load(key).unwrap().is_none());
        }
        assert_eq!(store.load().unwrap().settings(), &settings);
    }

    #[test]
    fn settings_document_cannot_serialize_sensitive_or_arbitrary_fields() {
        assert!(matches!(
            FileSettingsStore::new("relative/path"),
            Err(PersistenceError::InvalidStoragePath)
        ));
        let serialized = serde_json::to_string(&AppSettings::default()).unwrap();
        for forbidden in [
            "token",
            "secret",
            "credential",
            "subscription",
            "bootstrap",
            "url",
            "host",
            "path",
        ] {
            assert!(!serialized.to_ascii_lowercase().contains(forbidden));
        }
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value["token"] = Value::String("forbidden".to_owned());
        assert!(serde_json::from_value::<AppSettings>(value).is_err());
    }

    fn write_document(store: &FileSettingsStore, generation: u64, settings: Value) {
        store.ensure_directory().unwrap();
        let document = serde_json::json!({
            "storageVersion": STORAGE_FORMAT_VERSION,
            "generation": generation,
            "settings": settings
        });
        fs::write(
            store.directory().join(file_name(generation)),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
    }

    fn committed_files(store: &FileSettingsStore) -> Vec<u64> {
        let mut generations = store
            .candidate_files()
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.generation)
            .collect::<Vec<_>>();
        generations.sort_unstable();
        generations
    }
}
