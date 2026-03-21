use ip_network_table::IpNetworkTable;
use ip_network::IpNetwork;
use std::net::IpAddr;
use std::sync::Arc;
use tracing::{debug, error, warn};

use crate::config::IpAccessControl;

#[derive(Clone)]
pub struct IpFilter {
    pub enabled: bool,
    default_action: String,
    whitelist: Arc<IpNetworkTable<()>>,
    blacklist: Arc<IpNetworkTable<()>>,
}

impl IpFilter {
    pub fn new(config: &IpAccessControl) -> Self {
        if !config.enabled {
            return Self {
                enabled: false,
                default_action: String::new(),
                whitelist: Arc::new(IpNetworkTable::new()),
                blacklist: Arc::new(IpNetworkTable::new()),
            };
        }

        let parse_nets = |nets: &[String], list_name: &str| -> Arc<IpNetworkTable<()>> {
            let mut table = IpNetworkTable::new();
            for s in nets {
                let net = match s.parse::<IpNetwork>() {
                    Ok(net) => net,
                    Err(_) => match s.parse::<IpAddr>() {
                        Ok(ip) => IpNetwork::from(ip),
                        Err(e) => {
                            error!("Failed to parse IP/CIDR in {}: '{}' - {}", list_name, s, e);
                            continue;
                        }
                    },
                };
                table.insert(net, ());
            }
            Arc::new(table)
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
        if self.whitelist.longest_match(*ip).is_some() {
            debug!("IP {} allowed by whitelist", ip);
            return true;
        }

        // 2. Blacklist check
        if self.blacklist.longest_match(*ip).is_some() {
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
