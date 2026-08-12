use std::collections::BTreeSet;

pub(crate) fn route_table_has_interface(table: &str, name: &str) -> bool {
    table.lines().any(|line| {
        line.split_ascii_whitespace()
            .last()
            .is_some_and(|interface| interface == name)
    })
}

pub(crate) fn route_table_captures_family(table: &str, name: &str, ipv6: bool) -> bool {
    let destinations = table.lines().filter_map(|line| {
        let columns = line.split_ascii_whitespace().collect::<Vec<_>>();
        let destination = columns.first().copied()?;
        let interface = columns.last().copied()?;
        (interface == name).then_some(destination)
    });
    let destinations = destinations.collect::<BTreeSet<_>>();
    if destinations.contains("default") {
        return true;
    }
    if ipv6 {
        destinations.contains("::/1") && destinations.contains("8000::/1")
    } else {
        (destinations.contains("0/1") || destinations.contains("0.0.0.0/1"))
            && (destinations.contains("128/1") || destinations.contains("128.0.0.0/1"))
    }
}

pub(crate) fn dns_has_scoped_resolver(output: &str, name: &str) -> bool {
    output.split("resolver #").any(|resolver| {
        dns_has_valid_nameserver(resolver) && dns_mentions_interface(resolver, name)
    })
}

fn dns_has_valid_nameserver(resolver: &str) -> bool {
    resolver.lines().any(|line| {
        line.split_once(':').is_some_and(|(key, value)| {
            key.trim_start().starts_with("nameserver[")
                && value.trim().parse::<std::net::IpAddr>().is_ok()
        })
    })
}

pub(crate) fn dns_mentions_interface(output: &str, name: &str) -> bool {
    output.lines().any(|line| {
        line.split_ascii_whitespace()
            .any(|token| token.trim_matches(|value: char| !value.is_ascii_alphanumeric()) == name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_table_requires_interface_column_match() {
        let table = "Destination Gateway Flags Netif Expire\ndefault link#22 UCSg utun7\n";
        assert!(route_table_has_interface(table, "utun7"));
        assert!(!route_table_has_interface(table, "utun"));
        assert!(!route_table_has_interface(table, "utun8"));
    }

    #[test]
    fn tun_readiness_requires_complete_capture_routes_for_both_families() {
        let ipv4 = "Destination Gateway Flags Netif Expire\n0/1 link#22 UCSg utun7\n128/1 link#22 UCSg utun7\n";
        let ipv6 = "Destination Gateway Flags Netif Expire\n::/1 fe80::1 UGc utun7\n8000::/1 fe80::1 UGc utun7\n";
        assert!(route_table_captures_family(ipv4, "utun7", false));
        assert!(route_table_captures_family(ipv6, "utun7", true));
        assert!(!route_table_captures_family(
            "0/1 link#22 UCSg utun7\n",
            "utun7",
            false
        ));
        assert!(!route_table_captures_family(
            "172.19.0/30 link#22 UCS utun7\n",
            "utun7",
            false
        ));
    }

    #[test]
    fn dns_parser_requires_a_valid_nameserver_on_the_scoped_interface() {
        let output = "resolver #1\n  nameserver[0] : 192.0.2.53\n  if_index : 4 (en0)\nresolver #2\n  nameserver[0] : fdfe:dcba:9876::2\n  if_index : 22 (utun7)\n";
        assert!(dns_has_scoped_resolver(output, "utun7"));
        assert!(dns_mentions_interface(output, "utun7"));
        assert!(!dns_has_scoped_resolver(
            "resolver #1\n  nameserver[0] : 192.0.2.53\n  if_index : 4 (en0)\nresolver #2\n  nameserver[0] : invalid\n  if_index : 22 (utun7)\n",
            "utun7"
        ));
    }
}
