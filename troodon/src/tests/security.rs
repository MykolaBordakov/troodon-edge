use crate::config::IpAccessControl;
use crate::security::IpFilter;
use std::net::IpAddr;

fn make_filter(default_action: &str, whitelist: Vec<&str>, blacklist: Vec<&str>) -> IpFilter {
    IpFilter::new(&IpAccessControl {
        enabled: true,
        default_action: default_action.to_string(),
        whitelist: whitelist.iter().map(|s| s.to_string()).collect(),
        blacklist: blacklist.iter().map(|s| s.to_string()).collect(),
    })
}

// --- IPv4 tests ---

#[test]
fn test_ipv4_allowed_by_whitelist() {
    let f = make_filter("deny", vec!["192.168.1.10"], vec![]);
    let ip: IpAddr = "192.168.1.10".parse().unwrap();
    assert!(f.is_allowed(&ip));
}

#[test]
fn test_ipv4_blocked_by_blacklist() {
    let f = make_filter("allow", vec![], vec!["10.0.0.1"]);
    let ip: IpAddr = "10.0.0.1".parse().unwrap();
    assert!(!f.is_allowed(&ip));
}

#[test]
fn test_ipv4_cidr_whitelist() {
    let f = make_filter("deny", vec!["192.168.0.0/16"], vec![]);
    let ip: IpAddr = "192.168.5.100".parse().unwrap();
    assert!(f.is_allowed(&ip));
}

#[test]
fn test_ipv4_default_deny() {
    let f = make_filter("deny", vec![], vec![]);
    let ip: IpAddr = "1.2.3.4".parse().unwrap();
    assert!(!f.is_allowed(&ip));
}

#[test]
fn test_ipv4_default_allow() {
    let f = make_filter("allow", vec![], vec![]);
    let ip: IpAddr = "1.2.3.4".parse().unwrap();
    assert!(f.is_allowed(&ip));
}

// --- IPv6 tests (Fix 2: IPv6 bypass) ---

#[test]
fn test_ipv6_blocked_by_blacklist() {
    let f = make_filter("allow", vec![], vec!["2001:db8::1"]);
    let ip: IpAddr = "2001:db8::1".parse().unwrap();
    assert!(!f.is_allowed(&ip));
}

#[test]
fn test_ipv6_allowed_by_whitelist() {
    let f = make_filter("deny", vec!["2001:db8::42"], vec![]);
    let ip: IpAddr = "2001:db8::42".parse().unwrap();
    assert!(f.is_allowed(&ip));
}

#[test]
fn test_ipv6_cidr_whitelist() {
    // ::1/128 tightly matches only ::1
    let f = make_filter("deny", vec!["::1/128"], vec![]);
    let ip: IpAddr = "::1".parse().unwrap();
    assert!(f.is_allowed(&ip));
}

#[test]
fn test_ipv6_default_deny() {
    let f = make_filter("deny", vec![], vec![]);
    let ip: IpAddr = "2001:db8::ff".parse().unwrap();
    assert!(!f.is_allowed(&ip));
}

// --- Disabled filter ---

#[test]
fn test_disabled_filter_allows_all() {
    let f = IpFilter::new(&IpAccessControl {
        enabled: false,
        default_action: "deny".to_string(),
        whitelist: vec![],
        blacklist: vec!["0.0.0.0/0".to_string()],
    });
    let ip: IpAddr = "1.2.3.4".parse().unwrap();
    assert!(f.is_allowed(&ip));
}

// --- Whitelist priority over blacklist ---

#[test]
fn test_whitelist_takes_priority_over_blacklist() {
    // Same IP in both lists — whitelist wins
    let f = make_filter("deny", vec!["192.168.1.5"], vec!["192.168.1.5"]);
    let ip: IpAddr = "192.168.1.5".parse().unwrap();
    assert!(f.is_allowed(&ip));
}
