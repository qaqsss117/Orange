use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read as _, Write as _},
    net::{IpAddr, SocketAddr},
    path::Path,
    time::{Duration, Instant},
};

use orange_bootstrap::valid_https_url;
use reqwest::blocking::Client;
use sha2::{Digest as _, Sha256};

const DNS_RESPONSE_LIMIT: usize = 32 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct PinnedHttpsClient {
    resolvers: Vec<IpAddr>,
    user_agent: String,
}

impl PinnedHttpsClient {
    pub(crate) fn new(resolvers: &[IpAddr], user_agent: String) -> Option<Self> {
        if resolvers.len() < 2 || resolvers.iter().any(|resolver| !public_ip(*resolver)) {
            return None;
        }
        Some(Self {
            resolvers: resolvers.to_vec(),
            user_agent,
        })
    }

    pub(crate) fn get_bounded(
        &self,
        url: &str,
        deadline: Instant,
        limit: usize,
    ) -> Option<Vec<u8>> {
        if !valid_https_url(url) || limit == 0 {
            return None;
        }
        let parsed = reqwest::Url::parse(url).ok()?;
        let host = parsed.host_str()?.to_ascii_lowercase();
        let addresses = match host.parse::<IpAddr>() {
            Ok(address) if public_ip(address) => vec![address],
            Ok(_) => return None,
            Err(_) => self.resolve_public_addresses(&host, deadline),
        };
        for address in addresses {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            if remaining.is_zero() {
                return None;
            }
            let client = Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(CONNECT_TIMEOUT.min(remaining))
                .timeout(remaining)
                .resolve(&host, SocketAddr::new(address, 443))
                .user_agent(&self.user_agent)
                .build()
                .ok()?;
            let response = match client.get(url).send() {
                Ok(response) => response,
                Err(_) => continue,
            };
            if !response.status().is_success()
                || response.remote_addr().map(|peer| peer.ip()) != Some(address)
            {
                continue;
            }
            if let Some(bytes) = read_bounded(response, limit) {
                return Some(bytes);
            }
        }
        None
    }

    pub(crate) fn txt_records(&self, names: &[String], deadline: Instant) -> Vec<String> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            for name in names {
                for resolver in &self.resolvers {
                    let sender = sender.clone();
                    scope.spawn(move || {
                        let records =
                            query_dns(*resolver, name, "TXT", deadline).unwrap_or_default();
                        let _ = sender.send(records);
                    });
                }
            }
        });
        drop(sender);
        receiver.into_iter().flatten().collect()
    }

    pub(crate) fn download_verified(
        &self,
        url: &str,
        deadline: Instant,
        destination: &Path,
        expected_bytes: u64,
        expected_sha256: &str,
    ) -> bool {
        if !valid_https_url(url)
            || expected_bytes == 0
            || expected_sha256.len() != 64
            || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return false;
        }
        let Some(parsed) = reqwest::Url::parse(url).ok() else {
            return false;
        };
        let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else {
            return false;
        };
        let addresses = match host.parse::<IpAddr>() {
            Ok(address) if public_ip(address) => vec![address],
            Ok(_) => return false,
            Err(_) => self.resolve_public_addresses(&host, deadline),
        };
        for address in addresses {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let Some(client) = Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(CONNECT_TIMEOUT.min(remaining))
                .timeout(remaining)
                .resolve(&host, SocketAddr::new(address, 443))
                .user_agent(&self.user_agent)
                .build()
                .ok()
            else {
                continue;
            };
            let Ok(response) = client.get(url).send() else {
                continue;
            };
            if !response.status().is_success()
                || response.remote_addr().map(|peer| peer.ip()) != Some(address)
                || response
                    .content_length()
                    .is_some_and(|length| length != expected_bytes)
            {
                continue;
            }
            if write_verified(response, destination, expected_bytes, expected_sha256) {
                return true;
            }
        }
        let _ = fs::remove_file(destination);
        false
    }

    fn resolve_public_addresses(&self, host: &str, deadline: Instant) -> Vec<IpAddr> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            for resolver in &self.resolvers {
                for record_type in ["A", "AAAA"] {
                    let sender = sender.clone();
                    scope.spawn(move || {
                        let addresses = query_dns(*resolver, host, record_type, deadline)
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|value| value.parse::<IpAddr>().ok())
                            .filter(|address| public_ip(*address))
                            .collect::<Vec<_>>();
                        let _ = sender.send(addresses);
                    });
                }
            }
        });
        drop(sender);
        let mut unique = HashSet::new();
        receiver
            .into_iter()
            .flatten()
            .filter(|address| unique.insert(*address))
            .collect()
    }
}

fn query_dns(
    resolver: IpAddr,
    name: &str,
    record_type: &str,
    deadline: Instant,
) -> Option<Vec<String>> {
    let remaining = deadline.checked_duration_since(Instant::now())?;
    let (host, endpoint) = resolver_endpoint(resolver);
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT.min(remaining))
        .timeout(remaining)
        .resolve(host, SocketAddr::new(resolver, 443))
        .build()
        .ok()?;
    let response = client
        .get(endpoint)
        .query(&[("name", name), ("type", record_type)])
        .header("accept", "application/dns-json")
        .send()
        .ok()?;
    if !response.status().is_success()
        || response.remote_addr().map(|peer| peer.ip()) != Some(resolver)
    {
        return None;
    }
    let bytes = read_bounded(response, DNS_RESPONSE_LIMIT)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    Some(
        value
            .get("Answer")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|answer| answer.get("data").and_then(serde_json::Value::as_str))
            .map(str::to_owned)
            .collect(),
    )
}

fn resolver_endpoint(resolver: IpAddr) -> (&'static str, &'static str) {
    match resolver {
        IpAddr::V4(address) if matches!(address.octets(), [1, 1, 1, 1] | [1, 0, 0, 1]) => {
            ("cloudflare-dns.com", "https://cloudflare-dns.com/dns-query")
        }
        _ => ("dns.google", "https://dns.google/resolve"),
    }
}

fn read_bounded(response: reqwest::blocking::Response, limit: usize) -> Option<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return None;
    }
    let mut bytes = Vec::with_capacity(limit.min(16 * 1024));
    response
        .take(u64::try_from(limit).ok()?.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

fn write_verified(
    mut response: reqwest::blocking::Response,
    destination: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
) -> bool {
    let result = (|| -> Option<()> {
        let mut file = File::create(destination).ok()?;
        let mut digest = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = response.read(&mut buffer).ok()?;
            if count == 0 {
                break;
            }
            total = total.checked_add(u64::try_from(count).ok()?)?;
            if total > expected_bytes {
                return None;
            }
            digest.update(&buffer[..count]);
            file.write_all(&buffer[..count]).ok()?;
        }
        buffer.fill(0);
        file.sync_all().ok()?;
        let actual = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        (total == expected_bytes && actual.eq_ignore_ascii_case(expected_sha256)).then_some(())
    })();
    if result.is_none() {
        let _ = fs::remove_file(destination);
    }
    result.is_some()
}

pub(crate) fn public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let octets = address.octets();
            !matches!(
                octets,
                [0, ..]
                    | [10, ..]
                    | [100, 64..=127, ..]
                    | [127, ..]
                    | [169, 254, ..]
                    | [172, 16..=31, ..]
                    | [192, 0, 0, ..]
                    | [192, 0, 2, ..]
                    | [192, 168, ..]
                    | [198, 18..=19, ..]
                    | [198, 51, 100, ..]
                    | [203, 0, 113, ..]
                    | [224..=255, ..]
            )
        }
        IpAddr::V6(address) => {
            let segments = address.segments();
            !address.is_unspecified()
                && !address.is_loopback()
                && segments[0] & 0xfe00 != 0xfc00
                && segments[0] & 0xffc0 != 0xfe80
                && segments[0] & 0xff00 != 0xff00
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::public_ip;

    #[test]
    fn private_and_documentation_addresses_are_rejected() {
        assert!(!public_ip("127.0.0.1".parse().expect("IP")));
        assert!(!public_ip("192.168.1.1".parse().expect("IP")));
        assert!(!public_ip("203.0.113.8".parse().expect("IP")));
        assert!(public_ip("1.1.1.1".parse().expect("IP")));
    }
}
