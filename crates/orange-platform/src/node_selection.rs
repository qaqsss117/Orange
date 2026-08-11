//! Platform-neutral automatic node selection.
//!
//! The algorithm groups selectable nodes by the capacity group reported by the
//! panel, drops overloaded machines while healthy ones remain, probes a bounded
//! number of candidates, and finally picks a node with weighted rendezvous
//! hashing so that a given installation sticks to a stable choice while the
//! fleet stays balanced in aggregate.
//!
//! Delay probing is supplied by the caller so that this module stays free of
//! any transport or operating-system dependency.

use std::collections::{BTreeMap, HashMap};

use orange_domain::{NodeLoad, NodeLoadState, NodeLoadsResponse, PublicNodeProtocol};
use sha2::{Digest, Sha256};

use crate::data_plane_nodes::{SelectableNodeProtocol, SelectorGroup};

pub const AUTOMATIC_PROBE_TIMEOUT_MS: u64 = 1_500;
const MAX_AUTOMATIC_MACHINES: usize = 5;
const MAX_AUTOMATIC_PROBES: usize = 8;

/// Maps an internal selectable protocol onto the public IPC protocol.
pub const fn map_node_protocol(protocol: SelectableNodeProtocol) -> PublicNodeProtocol {
    match protocol {
        SelectableNodeProtocol::Shadowsocks => PublicNodeProtocol::Shadowsocks,
        SelectableNodeProtocol::Trojan => PublicNodeProtocol::Trojan,
        SelectableNodeProtocol::Hysteria2 => PublicNodeProtocol::Hysteria2,
        SelectableNodeProtocol::Vless => PublicNodeProtocol::Vless,
    }
}

/// Returns the still-valid load entries of `snapshot`, keyed by node id.
///
/// Both the snapshot as a whole and each individual entry must be within the
/// server-provided TTL; anything staler is dropped so that selection falls back
/// to delay-only probing rather than acting on outdated capacity data.
pub fn fresh_loads(snapshot: Option<&NodeLoadsResponse>, now: u64) -> HashMap<String, NodeLoad> {
    let Some(snapshot) = snapshot else {
        return HashMap::new();
    };
    if now > snapshot.generated_at.saturating_add(snapshot.ttl_seconds) {
        return HashMap::new();
    }
    snapshot
        .nodes
        .iter()
        .filter(|load| {
            load.state == NodeLoadState::Unknown
                || load.updated_at.is_some_and(|updated_at| {
                    now <= updated_at.saturating_add(snapshot.ttl_seconds)
                })
        })
        .cloned()
        .map(|load| (load.id.clone(), load))
        .collect()
}

/// Picks the node `group` should switch to in automatic mode.
///
/// `probe_delays` receives the bounded candidate list and the probe timeout in
/// milliseconds, and returns the reachable nodes with their measured delay. It
/// is invoked exactly once.
pub fn select_automatic_node(
    group: &SelectorGroup,
    loads: &HashMap<String, NodeLoad>,
    installation_id: &str,
    probe_delays: impl FnOnce(&[String], u64) -> HashMap<String, u32>,
) -> String {
    let mut machines = BTreeMap::<String, MachineCandidate>::new();
    for node in group.nodes() {
        let Some(load) = loads.get(node.id()) else {
            continue;
        };
        let Some(load_value) = load.load else {
            continue;
        };
        let candidate = machines
            .entry(load.capacity_group.clone())
            .or_insert_with(|| MachineCandidate {
                capacity_group: load.capacity_group.clone(),
                load: load_value,
                weight: load.selection_weight,
                overloaded: load.state == NodeLoadState::Overloaded,
                node_ids: Vec::new(),
            });
        candidate.load = candidate.load.min(load_value);
        candidate.node_ids.push(node.id().to_owned());
    }

    if machines.is_empty() {
        return select_by_delay_only(group, installation_id, probe_delays);
    }
    let mut machines = eligible_machines(machines.into_values().collect());

    let mut targets = Vec::new();
    for machine in &mut machines {
        machine.node_ids.sort_by_key(|node_id| {
            stable_hash(&[installation_id, &machine.capacity_group, node_id])
        });
        if let Some(node_id) = machine.node_ids.first() {
            targets.push(node_id.clone());
        }
    }
    for machine in &machines {
        for node_id in machine.node_ids.iter().skip(1) {
            if targets.len() == MAX_AUTOMATIC_PROBES {
                break;
            }
            targets.push(node_id.clone());
        }
    }
    let delays = probe_delays(&targets, AUTOMATIC_PROBE_TIMEOUT_MS);
    if delays.is_empty() {
        return group.default_node_id().to_owned();
    }

    let mut scored = Vec::new();
    for machine in machines {
        let mut nodes = machine
            .node_ids
            .iter()
            .filter_map(|node_id| {
                delays.get(node_id).map(|delay_ms| {
                    let score = node_score(machine.load, *delay_ms);
                    (node_id.clone(), score)
                })
            })
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            continue;
        }
        nodes.sort_by(|left, right| {
            left.1.total_cmp(&right.1).then_with(|| {
                stable_hash(&[installation_id, &left.0])
                    .cmp(&stable_hash(&[installation_id, &right.0]))
            })
        });
        scored.push(ScoredMachine {
            capacity_group: machine.capacity_group,
            weight: machine.weight,
            score: nodes[0].1,
            node_id: nodes[0].0.clone(),
        });
    }
    if scored.is_empty() {
        return group.default_node_id().to_owned();
    }
    let best_score = scored
        .iter()
        .map(|candidate| candidate.score)
        .min_by(f64::total_cmp)
        .unwrap_or(1.0);
    scored.retain(|candidate| candidate.score <= best_score + 0.10);
    scored.sort_by(|left, right| {
        weighted_rendezvous_rank(installation_id, left)
            .total_cmp(&weighted_rendezvous_rank(installation_id, right))
    });

    scored[0].node_id.clone()
}

fn select_by_delay_only(
    group: &SelectorGroup,
    installation_id: &str,
    probe_delays: impl FnOnce(&[String], u64) -> HashMap<String, u32>,
) -> String {
    let targets = group
        .nodes()
        .iter()
        .take(MAX_AUTOMATIC_PROBES)
        .map(|node| node.id().to_owned())
        .collect::<Vec<_>>();
    let delays = probe_delays(&targets, AUTOMATIC_PROBE_TIMEOUT_MS);
    delays
        .into_iter()
        .min_by(|left, right| {
            left.1.cmp(&right.1).then_with(|| {
                stable_hash(&[installation_id, &left.0])
                    .cmp(&stable_hash(&[installation_id, &right.0]))
            })
        })
        .map_or_else(
            || group.default_node_id().to_owned(),
            |(node_id, _)| node_id,
        )
}

/// Derives the per-installation jitter for the node load refresh cadence.
pub fn load_refresh_interval_seconds(installation_id: &str) -> u64 {
    50 + stable_hash(&[installation_id, "refresh"]) % 21
}

struct MachineCandidate {
    capacity_group: String,
    load: f64,
    weight: f64,
    overloaded: bool,
    node_ids: Vec<String>,
}

struct ScoredMachine {
    capacity_group: String,
    weight: f64,
    score: f64,
    node_id: String,
}

fn eligible_machines(mut machines: Vec<MachineCandidate>) -> Vec<MachineCandidate> {
    let has_healthy = machines.iter().any(|candidate| !candidate.overloaded);
    if has_healthy {
        machines.retain(|candidate| !candidate.overloaded);
    }
    machines.sort_by(|left, right| {
        left.load
            .total_cmp(&right.load)
            .then_with(|| left.capacity_group.cmp(&right.capacity_group))
    });
    machines.truncate(if has_healthy {
        MAX_AUTOMATIC_MACHINES
    } else {
        1
    });
    machines
}

fn node_score(load: f64, delay_ms: u32) -> f64 {
    0.65 * load + 0.35 * (f64::from(delay_ms) / 500.0).min(1.0)
}

fn stable_hash(parts: &[&str]) -> u64 {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 prefix is eight bytes"),
    )
}

fn weighted_rendezvous_rank(installation_id: &str, candidate: &ScoredMachine) -> f64 {
    let hash = stable_hash(&[installation_id, &candidate.capacity_group]);
    let uniform = (hash as f64 + 1.0) / (u64::MAX as f64 + 1.0);
    -uniform.ln() / candidate.weight
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(group: &str, load: f64, overloaded: bool) -> MachineCandidate {
        MachineCandidate {
            capacity_group: group.to_owned(),
            load,
            weight: 1.0,
            overloaded,
            node_ids: vec![format!("{group}-node")],
        }
    }

    fn scored(group: &str, weight: f64) -> ScoredMachine {
        ScoredMachine {
            capacity_group: group.to_owned(),
            weight,
            score: 0.4,
            node_id: format!("{group}-node"),
        }
    }

    fn load(id: &str, state: NodeLoadState, updated_at: Option<u64>) -> NodeLoad {
        NodeLoad {
            id: id.to_owned(),
            capacity_group: "m-1".to_owned(),
            load: (state != NodeLoadState::Unknown).then_some(0.5),
            state,
            updated_at,
            selection_weight: 1.0,
        }
    }

    fn snapshot(generated_at: u64, ttl_seconds: u64, nodes: Vec<NodeLoad>) -> NodeLoadsResponse {
        NodeLoadsResponse {
            schema_version: 1,
            generated_at,
            ttl_seconds,
            nodes,
        }
    }

    #[test]
    fn score_combines_load_and_capped_delay() {
        assert!((node_score(0.4, 250) - 0.435).abs() < 1e-12);
        assert!((node_score(0.4, 5_000) - 0.61).abs() < 1e-12);
        assert_eq!(AUTOMATIC_PROBE_TIMEOUT_MS, 1_500);
    }

    #[test]
    fn healthy_machines_exclude_overloaded_candidates() {
        let candidates = eligible_machines(vec![
            machine("m-overloaded", 0.1, true),
            machine("m-normal", 0.7, false),
        ]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].capacity_group, "m-normal");
    }

    #[test]
    fn all_overloaded_uses_lowest_load_machine_only() {
        let candidates = eligible_machines(vec![
            machine("m-high", 0.99, true),
            machine("m-low", 0.91, true),
            machine("m-mid", 0.95, true),
        ]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].capacity_group, "m-low");
    }

    #[test]
    fn rendezvous_is_stable_and_respects_weight_distribution() {
        let light = scored("m-light", 1.0);
        let heavy = scored("m-heavy", 4.0);
        let first = [
            weighted_rendezvous_rank("installation-a", &light),
            weighted_rendezvous_rank("installation-a", &heavy),
        ];
        let second = [
            weighted_rendezvous_rank("installation-a", &light),
            weighted_rendezvous_rank("installation-a", &heavy),
        ];
        assert_eq!(first, second);

        let heavy_wins = (0..2_000)
            .filter(|index| {
                let installation_id = format!("installation-{index}");
                weighted_rendezvous_rank(&installation_id, &heavy)
                    < weighted_rendezvous_rank(&installation_id, &light)
            })
            .count();
        assert!(heavy_wins > 1_400, "heavy candidate won {heavy_wins} times");
    }

    #[test]
    fn missing_snapshot_yields_no_loads() {
        assert!(fresh_loads(None, 1_000).is_empty());
    }

    #[test]
    fn expired_snapshot_is_discarded_wholesale() {
        let response = snapshot(
            1_000,
            180,
            vec![load("xb-1", NodeLoadState::Normal, Some(1_000))],
        );
        assert!(fresh_loads(Some(&response), 1_181).is_empty());
        assert_eq!(fresh_loads(Some(&response), 1_180).len(), 1);
    }

    #[test]
    fn stale_entries_are_dropped_but_unknown_entries_survive() {
        let response = snapshot(
            1_000,
            180,
            vec![
                load("xb-fresh", NodeLoadState::Normal, Some(1_000)),
                load("xb-stale", NodeLoadState::Normal, Some(800)),
                load("xb-unknown", NodeLoadState::Unknown, None),
            ],
        );
        let loads = fresh_loads(Some(&response), 1_100);
        assert!(loads.contains_key("xb-fresh"));
        assert!(!loads.contains_key("xb-stale"));
        assert!(loads.contains_key("xb-unknown"));
    }

    #[test]
    fn refresh_interval_is_stable_and_bounded() {
        for index in 0..500 {
            let installation_id = format!("installation-{index}");
            let interval = load_refresh_interval_seconds(&installation_id);
            assert!((50..=70).contains(&interval), "interval was {interval}");
            assert_eq!(interval, load_refresh_interval_seconds(&installation_id));
        }
    }
}
