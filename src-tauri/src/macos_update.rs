use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use orange_domain::{CommandError, ErrorCode, MacosPackageUpdateResponse};
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

const MAX_PACKAGE_BYTES: usize = 512 * 1024 * 1024;

pub async fn check(app: &tauri::AppHandle) -> Result<MacosPackageUpdateResponse, CommandError> {
    let update = app
        .updater()
        .map_err(service_error)?
        .check()
        .await
        .map_err(service_error)?;
    Ok(MacosPackageUpdateResponse::checked(
        update.map(|update| update.version),
    ))
}

pub async fn prepare(app: &tauri::AppHandle) -> Result<MacosPackageUpdateResponse, CommandError> {
    let update = app
        .updater()
        .map_err(service_error)?
        .check()
        .await
        .map_err(service_error)?
        .ok_or_else(|| CommandError::from_code(ErrorCode::Validation))?;
    let version = update.version.clone();
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(service_error)?;
    if bytes.is_empty() || bytes.len() > MAX_PACKAGE_BYTES {
        return Err(CommandError::from_code(ErrorCode::Validation));
    }
    let package = write_private_package(app, &bytes)?;
    verify_running_application()?;
    verify_package(&package)?;
    let reconnect = stop_data_plane_for_update(app)?;
    if let Err(error) = verified_status_with_path("/usr/bin/open", &["-a", "Installer"], &package) {
        restore_after_failed_installer_launch(app, reconnect);
        return Err(error);
    }
    let _ = app
        .state::<crate::connection_recovery::ConnectionRecovery>()
        .clear();
    let response = MacosPackageUpdateResponse::prepared(version);
    app.exit(0);
    Ok(response)
}

fn write_private_package(app: &tauri::AppHandle, bytes: &[u8]) -> Result<PathBuf, CommandError> {
    let root = app
        .path()
        .app_cache_dir()
        .map_err(service_error)?
        .join("package-update");
    match fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(CommandError::from_code(ErrorCode::Permission));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&root).map_err(service_error)?;
        }
        Err(error) => return Err(service_error(error)),
    }
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(service_error)?;
    let package = root.join("Orange.pkg");
    remove_regular(&package)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&package)
        .map_err(service_error)?;
    file.write_all(bytes).map_err(service_error)?;
    file.sync_all().map_err(service_error)?;
    Ok(package)
}

fn verify_running_application() -> Result<(), CommandError> {
    let team = expected_team_id()?;
    let output = verified_output(
        "/usr/bin/codesign",
        &["-dvvv", "--strict", "/Applications/Orange.app"],
    )?;
    if !output.contains(&format!("TeamIdentifier={team}")) {
        return Err(CommandError::from_code(ErrorCode::Permission));
    }
    verified_status(
        "/usr/bin/codesign",
        &["--verify", "--deep", "--strict", "/Applications/Orange.app"],
    )
}

fn verify_package(package: &Path) -> Result<(), CommandError> {
    let package = package
        .to_str()
        .ok_or_else(|| CommandError::from_code(ErrorCode::Validation))?;
    let team = expected_team_id()?;
    let output = verified_output("/usr/sbin/pkgutil", &["--check-signature", package])?;
    if !output.contains("Developer ID Installer")
        || !output.contains(&format!("Team Identifier: {team}"))
    {
        return Err(CommandError::from_code(ErrorCode::Permission));
    }
    verified_status("/usr/sbin/spctl", &["-a", "-vv", "-t", "install", package])
}

fn stop_data_plane_for_update(app: &tauri::AppHandle) -> Result<bool, CommandError> {
    let control = app.state::<crate::planes::ManagedDataPlaneControl>();
    let planes = app.state::<crate::planes::ManagedPlanes>();
    let status = control.execute(orange_domain::DataPlaneControlAction::Status, &planes)?;
    let reconnect = status.data_plane == orange_domain::DataPlaneState::Online;
    control.begin_shutdown();
    if let Err(error) = control.execute_shutdown_stop(&planes) {
        control.cancel_shutdown();
        return Err(error);
    }
    if crate::macos_node_runtime::clear_connection_recovery().is_err() {
        restore_after_failed_installer_launch(app, reconnect);
        return Err(CommandError::from_code(ErrorCode::Service));
    }
    Ok(reconnect)
}

fn restore_after_failed_installer_launch(app: &tauri::AppHandle, reconnect: bool) {
    let control = app.state::<crate::planes::ManagedDataPlaneControl>();
    control.cancel_shutdown();
    if !reconnect {
        return;
    }
    let node_runtime = app.state::<Arc<dyn orange_platform::NodeRuntimeHost>>();
    let planes = app.state::<crate::planes::ManagedPlanes>();
    if node_runtime.prepare_auto_selection().is_ok() {
        let _ = control.execute(orange_domain::DataPlaneControlAction::Start, &planes);
    }
}

fn verified_status(program: &str, args: &[&str]) -> Result<(), CommandError> {
    let output = Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .output()
        .map_err(service_error)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CommandError::from_code(ErrorCode::Permission))
    }
}

fn verified_status_with_path(
    program: &str,
    args: &[&str],
    path: &Path,
) -> Result<(), CommandError> {
    let status = Command::new(program)
        .args(args)
        .arg(path)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(service_error)?;
    if status.success() {
        Ok(())
    } else {
        Err(CommandError::from_code(ErrorCode::Service))
    }
}

fn verified_output(program: &str, args: &[&str]) -> Result<String, CommandError> {
    let output = Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .output()
        .map_err(service_error)?;
    if !output.status.success() {
        return Err(CommandError::from_code(ErrorCode::Permission));
    }
    let mut bytes = output.stdout;
    bytes.extend(output.stderr);
    String::from_utf8(bytes).map_err(service_error)
}

fn expected_team_id() -> Result<&'static str, CommandError> {
    option_env!("ORANGE_DEVELOPER_TEAM_ID")
        .filter(|team| {
            team.len() == 10
                && team
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        })
        .ok_or_else(|| CommandError::from_code(ErrorCode::Internal))
}

fn remove_regular(path: &Path) -> Result<(), CommandError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(service_error(error)),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(CommandError::from_code(ErrorCode::Permission));
    }
    fs::remove_file(path).map_err(service_error)
}

fn service_error(_: impl std::fmt::Display) -> CommandError {
    CommandError::from_code(ErrorCode::Service)
}
