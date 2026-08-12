#[cfg(target_os = "macos")]
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::Duration,
};

#[cfg(target_os = "macos")]
use orange_domain::DataPlaneState;
#[cfg(target_os = "macos")]
use orange_platform::DataPlaneEventBackend;

#[cfg(target_os = "macos")]
const SELECTION_RESTORE_INTERVAL: Duration = Duration::from_millis(500);

#[cfg(target_os = "macos")]
pub struct MacosSelectionRuntime {
    control: Arc<MonitorControl>,
    worker: Option<JoinHandle<()>>,
}

#[cfg(target_os = "macos")]
impl MacosSelectionRuntime {
    pub fn start(
        node_runtime: Arc<dyn orange_platform::NodeRuntimeHost>,
    ) -> Result<Self, std::io::Error> {
        let control = Arc::new(MonitorControl::default());
        let worker_control = Arc::clone(&control);
        let worker = thread::Builder::new()
            .name("orange-macos-selection-restore".to_owned())
            .spawn(move || {
                let mut restore = SelectionRestore::default();
                while !worker_control.is_stopping() {
                    match DataPlaneEventBackend::data_plane_snapshot(node_runtime.as_ref()) {
                        Ok(snapshot)
                            if snapshot.state() == DataPlaneState::Online
                                && snapshot.has_active_instance() =>
                        {
                            restore.run(node_runtime.as_ref(), snapshot.instance_id());
                        }
                        Ok(_) | Err(_) => restore.reset(),
                    }
                    if worker_control.wait_or_stopping(SELECTION_RESTORE_INTERVAL) {
                        break;
                    }
                }
            })?;
        Ok(Self {
            control,
            worker: Some(worker),
        })
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacosSelectionRuntime {
    fn drop(&mut self) {
        self.control.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
struct SelectionRestore {
    instance_id: Option<u64>,
    restored: bool,
    attempts: usize,
}

impl SelectionRestore {
    const MAX_ATTEMPTS: usize = 10;

    #[cfg(target_os = "macos")]
    fn run(&mut self, node_runtime: &dyn orange_platform::NodeRuntimeHost, instance_id: u64) {
        self.run_with(instance_id, || node_runtime.restore_selections().is_ok());
    }

    fn run_with(&mut self, instance_id: u64, restore: impl FnOnce() -> bool) {
        if self.instance_id != Some(instance_id) {
            self.instance_id = Some(instance_id);
            self.restored = false;
            self.attempts = 0;
        }
        if self.restored || self.attempts >= Self::MAX_ATTEMPTS {
            return;
        }
        self.attempts += 1;
        if restore() {
            self.restored = true;
        }
    }

    fn reset(&mut self) {
        self.instance_id = None;
        self.restored = false;
        self.attempts = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::SelectionRestore;

    #[test]
    fn restores_once_per_online_instance() {
        let mut state = SelectionRestore::default();
        let mut calls = 0;
        state.run_with(1, || {
            calls += 1;
            true
        });
        state.run_with(1, || {
            calls += 1;
            true
        });
        state.run_with(2, || {
            calls += 1;
            true
        });
        assert_eq!(calls, 2);
    }

    #[test]
    fn retries_are_bounded_and_reset_after_disconnect() {
        let mut state = SelectionRestore::default();
        let mut calls = 0;
        for _ in 0..20 {
            state.run_with(7, || {
                calls += 1;
                false
            });
        }
        assert_eq!(calls, SelectionRestore::MAX_ATTEMPTS);
        state.reset();
        state.run_with(7, || {
            calls += 1;
            true
        });
        assert_eq!(calls, SelectionRestore::MAX_ATTEMPTS + 1);
    }
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct MonitorControl {
    stopping: Mutex<bool>,
    changed: Condvar,
}

#[cfg(target_os = "macos")]
impl MonitorControl {
    fn is_stopping(&self) -> bool {
        *lock(&self.stopping)
    }

    fn wait_or_stopping(&self, timeout: Duration) -> bool {
        let stopping = lock(&self.stopping);
        if *stopping {
            return true;
        }
        let (stopping, _) = self
            .changed
            .wait_timeout_while(stopping, timeout, |stopping| !*stopping)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *stopping
    }

    fn stop(&self) {
        *lock(&self.stopping) = true;
        self.changed.notify_all();
    }
}

#[cfg(target_os = "macos")]
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
