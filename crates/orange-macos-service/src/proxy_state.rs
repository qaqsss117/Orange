use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const JOURNAL_SCHEMA_VERSION: u16 = 1;
const PROXY_HOST: &str = "127.0.0.1";
const MANAGED_FIELDS: [&str; 9] = [
    "HTTPEnable",
    "HTTPProxy",
    "HTTPPort",
    "HTTPSEnable",
    "HTTPSProxy",
    "HTTPSPort",
    "SOCKSEnable",
    "SOCKSProxy",
    "SOCKSPort",
];

pub type ManagedProxyDictionary = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyServiceSnapshot {
    service_id: String,
    original: ManagedProxyDictionary,
    applied: ManagedProxyDictionary,
}

impl ProxyServiceSnapshot {
    pub fn capture(
        service_id: impl Into<String>,
        original: ManagedProxyDictionary,
        port: u16,
    ) -> Option<Self> {
        let service_id = service_id.into();
        if !valid_service_id(&service_id) || !orange_domain::valid_proxy_port(port) {
            return None;
        }
        let mut applied = original.clone();
        for prefix in ["HTTP", "HTTPS", "SOCKS"] {
            applied.insert(format!("{prefix}Enable"), Value::from(1));
            applied.insert(format!("{prefix}Proxy"), Value::from(PROXY_HOST));
            applied.insert(format!("{prefix}Port"), Value::from(port));
        }
        Some(Self {
            service_id,
            original,
            applied,
        })
    }

    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    pub fn applied(&self) -> &ManagedProxyDictionary {
        &self.applied
    }

    pub fn restore_into(&self, current: &mut ManagedProxyDictionary) -> ProxyRestoreOutcome {
        let mut restored = 0_usize;
        let mut preserved = 0_usize;
        for field in MANAGED_FIELDS {
            let applied = self.applied.get(field);
            if current.get(field) != applied {
                preserved += 1;
                continue;
            }
            match self.original.get(field) {
                Some(value) => {
                    current.insert(field.to_owned(), value.clone());
                }
                None => {
                    current.remove(field);
                }
            }
            restored += 1;
        }
        match (restored, preserved) {
            (0, _) => ProxyRestoreOutcome::PreservedUserChanges,
            (_, 0) => ProxyRestoreOutcome::Restored,
            _ => ProxyRestoreOutcome::PartiallyRestored,
        }
    }

    fn validate(&self) -> bool {
        let Some(port) = self
            .applied
            .get("HTTPPort")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
        else {
            return false;
        };
        Self::capture(self.service_id.clone(), self.original.clone(), port)
            .is_some_and(|expected| expected.applied == self.applied)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyRecoveryJournal {
    schema_version: u16,
    services: Vec<ProxyServiceSnapshot>,
}

impl ProxyRecoveryJournal {
    pub fn new(services: Vec<ProxyServiceSnapshot>) -> Option<Self> {
        if services.is_empty() || services.len() > 128 {
            return None;
        }
        let unique = services
            .iter()
            .map(|service| service.service_id.as_str())
            .collect::<BTreeSet<_>>();
        if unique.len() != services.len() || services.iter().any(|service| !service.validate()) {
            return None;
        }
        Some(Self {
            schema_version: JOURNAL_SCHEMA_VERSION,
            services,
        })
    }

    pub fn services(&self) -> &[ProxyServiceSnapshot] {
        &self.services
    }

    pub fn validate(&self) -> bool {
        self.schema_version == JOURNAL_SCHEMA_VERSION && Self::new(self.services.clone()).is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyRestoreOutcome {
    Restored,
    PartiallyRestored,
    PreservedUserChanges,
}

fn valid_service_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROXY_PORT: u16 = orange_domain::DEFAULT_PROXY_PORT;

    fn capture() -> ProxyServiceSnapshot {
        ProxyServiceSnapshot::capture("service-1", original(), PROXY_PORT).unwrap()
    }

    fn original() -> ManagedProxyDictionary {
        BTreeMap::from([
            ("HTTPEnable".to_owned(), Value::from(0)),
            ("HTTPProxy".to_owned(), Value::from("old.proxy")),
            ("HTTPPort".to_owned(), Value::from(8080)),
            (
                "ExceptionsList".to_owned(),
                serde_json::json!(["localhost"]),
            ),
        ])
    }

    #[test]
    fn apply_preserves_complete_original_dictionary() {
        let snapshot = capture();
        assert_eq!(snapshot.applied()["HTTPProxy"], PROXY_HOST);
        assert_eq!(snapshot.applied()["HTTPSPort"], PROXY_PORT);
        assert_eq!(
            snapshot.applied()["ExceptionsList"],
            serde_json::json!(["localhost"])
        );
    }

    #[test]
    fn custom_port_is_recorded_and_validated() {
        let snapshot = ProxyServiceSnapshot::capture("service-1", original(), 40_123).unwrap();
        assert_eq!(snapshot.applied()["SOCKSPort"], 40_123);
        assert!(ProxyRecoveryJournal::new(vec![snapshot]).is_some());
        assert!(
            ProxyServiceSnapshot::capture(
                "service-1",
                original(),
                orange_domain::RESERVED_PROXY_PROBE_PORT,
            )
            .is_none()
        );
    }

    #[test]
    fn exact_orange_fields_are_restored_without_touching_unmanaged_fields() {
        let snapshot = capture();
        let mut current = snapshot.applied().clone();
        current.insert("ExceptionsList".to_owned(), serde_json::json!(["changed"]));
        assert_eq!(
            snapshot.restore_into(&mut current),
            ProxyRestoreOutcome::Restored
        );
        assert_eq!(current["HTTPProxy"], "old.proxy");
        assert_eq!(current["ExceptionsList"], serde_json::json!(["changed"]));
        assert!(!current.contains_key("HTTPSProxy"));
    }

    #[test]
    fn user_changes_to_one_managed_field_are_preserved() {
        let snapshot = capture();
        let mut current = snapshot.applied().clone();
        current.insert("HTTPProxy".to_owned(), Value::from("user.proxy"));
        assert_eq!(
            snapshot.restore_into(&mut current),
            ProxyRestoreOutcome::PartiallyRestored
        );
        assert_eq!(current["HTTPProxy"], "user.proxy");
        assert_eq!(current["HTTPPort"], 8080);
        assert!(!current.contains_key("HTTPSProxy"));
    }

    #[test]
    fn journal_rejects_duplicate_services_and_tampering() {
        let snapshot = capture();
        assert!(ProxyRecoveryJournal::new(vec![snapshot.clone(), snapshot]).is_none());
        let mut valid = ProxyRecoveryJournal::new(vec![capture()]).unwrap();
        valid.services[0]
            .applied
            .insert("HTTPPort".to_owned(), Value::from(1));
        assert!(!valid.validate());
    }
}
