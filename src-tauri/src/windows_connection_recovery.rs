use std::{
    fs::{self, OpenOptions},
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
};

const ACTIVE_CONNECTION_MARKER: &str = "connection-active";

pub struct WindowsConnectionRecovery {
    marker_path: PathBuf,
}

impl WindowsConnectionRecovery {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            marker_path: app_data_dir.join(ACTIVE_CONNECTION_MARKER),
        }
    }

    pub fn should_reconnect(&self) -> bool {
        self.marker_path.is_file()
    }

    pub fn mark_connected(&self) -> io::Result<()> {
        let parent = self
            .marker_path
            .parent()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "missing marker parent"))?;
        fs::create_dir_all(parent)?;
        let mut marker = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.marker_path)?;
        marker.write_all(b"connected\n")?;
        marker.sync_all()
    }

    pub fn clear(&self) -> io::Result<()> {
        match fs::remove_file(&self.marker_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}
