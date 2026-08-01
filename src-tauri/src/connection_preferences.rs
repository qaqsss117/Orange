use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use orange_domain::ConnectionMode;
use orange_platform::{FileSettingsStore, PersistenceError, SettingsStorage};

pub struct ConnectionPreferences {
    store: Arc<FileSettingsStore>,
    mode: RwLock<ConnectionMode>,
}

impl ConnectionPreferences {
    pub fn load(store: Arc<FileSettingsStore>) -> Result<Self, PersistenceError> {
        let mode = store.load()?.settings().connection_mode();
        Ok(Self {
            store,
            mode: RwLock::new(mode),
        })
    }

    pub fn mode(&self) -> ConnectionMode {
        *read(&self.mode)
    }

    pub fn set_mode(&self, mode: ConnectionMode) -> Result<bool, PersistenceError> {
        let mut active = write(&self.mode);
        if *active == mode {
            return Ok(false);
        }
        let mut settings = self.store.load()?.into_settings();
        settings.set_connection_mode(mode);
        self.store.save(&settings)?;
        *active = mode;
        Ok(true)
    }
}

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
