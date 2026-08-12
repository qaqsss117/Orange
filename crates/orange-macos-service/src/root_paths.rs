use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::Path,
};

use orange_platform::PlatformVpnError;

pub(crate) fn ensure_root_private_directory(path: &Path) -> Result<(), PlatformVpnError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_directory(path, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or(PlatformVpnError::InvalidConfiguration)?;
            let parent_metadata =
                fs::symlink_metadata(parent).map_err(|_| PlatformVpnError::Unavailable)?;
            if !parent_metadata.is_dir()
                || parent_metadata.file_type().is_symlink()
                || parent_metadata.uid() != 0
            {
                return Err(PlatformVpnError::PermissionDenied);
            }
            fs::DirBuilder::new()
                .mode(0o700)
                .create(path)
                .map_err(|_| PlatformVpnError::Unavailable)?;
            let metadata = fs::symlink_metadata(path).map_err(|_| PlatformVpnError::Unavailable)?;
            validate_directory(path, &metadata)
        }
        Err(_) => Err(PlatformVpnError::Unavailable),
    }
}

fn validate_directory(path: &Path, metadata: &fs::Metadata) -> Result<(), PlatformVpnError> {
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.uid() != 0 {
        return Err(PlatformVpnError::PermissionDenied);
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| PlatformVpnError::Unavailable)?;
    let verified = fs::symlink_metadata(path).map_err(|_| PlatformVpnError::Unavailable)?;
    if !verified.is_dir()
        || verified.file_type().is_symlink()
        || verified.uid() != 0
        || verified.permissions().mode() & 0o777 != 0o700
    {
        return Err(PlatformVpnError::PermissionDenied);
    }
    Ok(())
}
