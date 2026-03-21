use ipnet::IpNet;
use std::net::IpAddr;
use tracing::{debug, error, warn};

use crate::config::IpAccessControl;

#[derive(Clone, Debug)]
pub struct IpFilter {
    pub enabled: bool,
    default_action: String,
    whitelist: Vec<IpNet>,
    blacklist: Vec<IpNet>,
}

impl IpFilter {
    pub fn new(config: &IpAccessControl) -> Self {
        if !config.enabled {
            return Self {
                enabled: false,
                default_action: String::new(),
                whitelist: Vec::new(),
                blacklist: Vec::new(),
            };
        }

        let parse_nets = |nets: &[String], list_name: &str| -> Vec<IpNet> {
            nets.iter()
                .filter_map(|s| {
                    // Try to parse as a CIDR network ("192.168.1.0/24")
                    match s.parse::<IpNet>() {
                        Ok(net) => Some(net),
                        Err(_) => {
                            // If it's a single IP ("192.168.1.100"), convert it to /32 or /128
                            match s.parse::<IpAddr>() {
                                Ok(ip) => Some(IpNet::from(ip)),
                                Err(e) => {
                                    error!(
                                        "Failed to parse IP/CIDR in {}: '{}' - {}",
                                        list_name, s, e
                                    );
                                    None
                                }
                            }
                        }
                    }
                })
                .collect()
        };

        Self {
            enabled: true,
            default_action: config.default_action.to_lowercase(),
            whitelist: parse_nets(&config.whitelist, "whitelist"),
            blacklist: parse_nets(&config.blacklist, "blacklist"),
        }
    }

    /// Повертає `true`, якщо IP має доступ.
    /// Алгоритм:
    ///   - Якщо disabled => true
    ///   - Якщо whitelist matches => true
    ///   - Якщо blacklist matches => false
    ///   - Якщо нічого => залежить від default_action
    pub fn is_allowed(&self, ip: &IpAddr) -> bool {
        if !self.enabled {
            return true;
        }

        // 1. Whitelist has highest priority
        if self.whitelist.iter().any(|net| net.contains(ip)) {
            debug!("IP {} allowed by whitelist", ip);
            return true;
        }

        // 2. Blacklist check
        if self.blacklist.iter().any(|net| net.contains(ip)) {
            warn!("IP {} blocked by blacklist", ip);
            return false;
        }

        // 3. Default action
        let allowed = self.default_action == "allow";
        if allowed {
            debug!("IP {} allowed by default_action='allow'", ip);
        } else {
            warn!("IP {} blocked by default_action='deny'", ip);
        }

        allowed
    }
}
