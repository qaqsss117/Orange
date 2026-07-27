#[cfg(windows)]
fn main() {
    if orange_windows_service::windows_service_main().is_err() {
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(1);
}
