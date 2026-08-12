#[cfg(target_os = "macos")]
fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = match arguments.as_slice() {
        [] => orange_macos_service::run_helper().map_err(|error| error.to_string()),
        [argument] if argument == "--restore-proxy" => {
            orange_macos_service::restore_system_proxy().map_err(|error| error.to_string())
        }
        _ => Err("invalid arguments".to_owned()),
    };
    if result.is_err() {
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    std::process::exit(1);
}
