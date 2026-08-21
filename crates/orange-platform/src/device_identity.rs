//! Stable device identity used for account session grouping.
//!
//! The MAC address is preferred because it follows the physical installation
//! across application restarts and repeated logins.  Some environments hide
//! interface addresses, so a hostname-based value is used only as a fallback.

use std::process::Command;

pub(crate) fn device_identifier() -> Option<String> {
    mac_address()
        .map(|mac| mac.to_ascii_uppercase())
        .or_else(host_identifier)
}

#[cfg(target_os = "windows")]
fn mac_address() -> Option<String> {
    let output = Command::new("getmac")
        .args(["/fo", "csv", "/nh"])
        .output()
        .ok()?;
    find_mac(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "macos")]
fn mac_address() -> Option<String> {
    let output = Command::new("/sbin/ifconfig").args(["-a"]).output().ok()?;
    find_mac(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "linux")]
fn mac_address() -> Option<String> {
    let mut interfaces = std::fs::read_dir("/sys/class/net")
        .ok()?
        .flatten()
        .collect::<Vec<_>>();
    interfaces.sort_by_key(|entry| entry.file_name());
    for interface in interfaces {
        let name = interface.file_name();
        if name == "lo" {
            continue;
        }
        let address = std::fs::read_to_string(interface.path().join("address")).ok()?;
        if let Some(mac) = find_mac(&address) {
            return Some(mac);
        }
    }
    None
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn mac_address() -> Option<String> {
    None
}

fn find_mac(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    for start in 0..bytes.len() {
        if start + 17 > bytes.len() {
            break;
        }
        let candidate = &bytes[start..start + 17];
        let separator = candidate[2];
        if !matches!(separator, b':' | b'-')
            || candidate[5] != separator
            || candidate[8] != separator
            || candidate[11] != separator
            || candidate[14] != separator
            || !candidate
                .iter()
                .enumerate()
                .all(|(index, byte)| index % 3 == 2 || byte.is_ascii_hexdigit())
        {
            continue;
        }

        let first = u8::from_str_radix(std::str::from_utf8(&candidate[..2]).ok()?, 16).ok()?;
        if first & 1 != 0
            || candidate
                .iter()
                .all(|byte| *byte == b'0' || *byte == separator)
        {
            continue;
        }

        let mut mac = String::with_capacity(17);
        for (index, byte) in candidate.iter().enumerate() {
            mac.push(if index % 3 == 2 {
                ':'
            } else {
                char::from(*byte).to_ascii_uppercase()
            });
        }
        return Some(mac);
    }
    None
}

fn host_identifier() -> Option<String> {
    let host = std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| {
            Command::new("hostname")
                .output()
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        })?;
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    let mut identifier = String::from("host:");
    identifier.extend(host.chars().filter(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
    }));
    (identifier.len() > "host:".len()).then_some(identifier)
}

#[cfg(test)]
mod tests {
    use super::find_mac;

    #[test]
    fn parses_colon_and_hyphen_mac_addresses() {
        assert_eq!(
            find_mac("ether aa:bb:cc:dd:ee:ff"),
            Some("AA:BB:CC:DD:EE:FF".into())
        );
        assert_eq!(
            find_mac("\"aa-bb-cc-dd-ee-ff\",\"adapter\""),
            Some("AA:BB:CC:DD:EE:FF".into())
        );
    }

    #[test]
    fn rejects_invalid_or_multicast_addresses() {
        assert_eq!(find_mac("00:00:00:00:00:00"), None);
        assert_eq!(find_mac("01:bb:cc:dd:ee:ff"), None);
        assert_eq!(find_mac("aa:bb:cc:dd:ee"), None);
    }
}
