#[cfg(windows)]
fn main() {
    if let Err(error) = orange_windows_service::windows_installer_main() {
        std::process::exit(error.exit_code());
    }
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(1);
}
