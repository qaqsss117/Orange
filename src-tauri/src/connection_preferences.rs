use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

use orange_domain::{ConnectionMode, RoutingMode};
use orange_platform::{FileSettingsStore, PersistenceError, SettingsStorage};

pub struct ConnectionPreferences {
    store: Arc<FileSettingsStore>,
    mode: RwLock<ConnectionMode>,
    routing_mode: RwLock<RoutingMode>,
    proxy_port: RwLock<u16>,
    reconfiguration: Mutex<()>,
}

impl ConnectionPreferences {
    pub fn load(store: Arc<FileSettingsStore>) -> Result<Self, PersistenceError> {
        let loaded = store.load()?;
        let mode = loaded.settings().connection_mode();
        let routing_mode = loaded.settings().routing_mode();
        let proxy_port = loaded.settings().proxy_port();
        Ok(Self {
            store,
            mode: RwLock::new(mode),
            routing_mode: RwLock::new(routing_mode),
            proxy_port: RwLock::new(proxy_port),
            reconfiguration: Mutex::new(()),
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

    pub fn routing_mode(&self) -> RoutingMode {
        *read(&self.routing_mode)
    }

    pub fn set_routing_mode(&self, mode: RoutingMode) -> Result<bool, PersistenceError> {
        let mut active = write(&self.routing_mode);
        if *active == mode {
            return Ok(false);
        }
        let mut settings = self.store.load()?.into_settings();
        settings.set_routing_mode(mode);
        self.store.save(&settings)?;
        *active = mode;
        Ok(true)
    }

    pub fn proxy_port(&self) -> u16 {
        *read(&self.proxy_port)
    }

    pub fn set_proxy_port(&self, port: u16) -> Result<bool, PersistenceError> {
        if !orange_domain::valid_proxy_port(port) {
            return Err(PersistenceError::InvalidSettings);
        }
        let mut active = write(&self.proxy_port);
        if *active == port {
            return Ok(false);
        }
        let mut settings = self.store.load()?.into_settings();
        settings.set_proxy_port(port);
        self.store.save(&settings)?;
        *active = port;
        Ok(true)
    }

    pub fn begin_reconfiguration(&self) -> Result<MutexGuard<'_, ()>, ()> {
        match self.reconfiguration.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::Poisoned(error)) => Ok(error.into_inner()),
            Err(TryLockError::WouldBlock) => Err(()),
        }
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
