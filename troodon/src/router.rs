use crate::config;
use crate::proxy::{ProxyRoute, ProxyRouter};
use matchit::Router;
use pingora::lb::LoadBalancer;
use pingora::lb::health_check::HttpHealthCheck;
use std::sync::Arc;
use tracing::{error, info};

pub fn build_router(conf: &config::Config) -> Option<ProxyRouter> {
    let mut router = Router::new();
    let mut health_checks = Vec::new();
    let mut route_count = 0;

    for route_conf in &conf.routes {
        let sni_host = route_conf.host.clone();

        for loc in &route_conf.locations {
            if loc.upstreams.is_empty() {
                error!(
                    "Location '{}' has no upstreams defined! Skipping.",
                    loc.path
                );
                continue;
            }

            // --- СТВОРЕННЯ БАЛАНСУВАЛЬНИКА З HEALTH CHECKS ---
            let mut lb = match LoadBalancer::try_from_iter(&loc.upstreams) {
                Ok(lb) => lb,
                Err(e) => {
                    error!("Failed to init LB for {}: {}", loc.path, e);
                    continue;
                }
            };

            // Вмикаємо Health Checking у фоні (Active Health Check)
            if let Some(ref hc_path) = loc.health_check_path {
                // Визначаємо TLS для health check аналогічно proxy.rs
                let hc_tls = loc.upstream_tls.unwrap_or(false);
                let mut hc = HttpHealthCheck::new(&sni_host, hc_tls);
                // Налаштовуємо шлях health check
                if let Ok(uri) = hc_path.parse() {
                    hc.req.set_uri(uri);
                }
                lb.set_health_check(Box::new(hc));
                info!(
                    "🔬 Enabled HTTP Health Check (path: '{}') for Location: {}",
                    hc_path, loc.path
                );
            }

            // Завертаємо у Arc після конфігурації
            let lb_arc = Arc::new(lb);

            // Якщо увімкнено Health Checks, зберігаємо його для BackgroundService
            if loc.health_check_path.is_some() {
                health_checks.push((loc.path.clone(), lb_arc.clone()));
            }

            // Визначаємо Timeouts для локації: беремо з локації або фолбеком глобальні
            let loc_timeouts = loc
                .timeouts
                .clone()
                .unwrap_or_else(|| conf.server.timeouts.clone());

            let proxy_route = Arc::new(ProxyRoute {
                path: loc.path.clone(),
                lb: lb_arc.clone(),
                sni: sni_host.clone(),
                host_header: loc.host_header.clone(),
                strip_prefix: loc.strip_prefix,
                max_inflight: loc.max_inflight,
                timeouts: loc_timeouts,
                retry_count: loc.retry_count,
                upstream_tls: loc.upstream_tls,
                websocket: loc.websocket,
                client_max_body_size: loc.client_max_body_size,
            });

            let path = loc.path.clone();

            // Якщо у нас налаштований точний збіг (exact_match = true), ми НЕ додаємо `{*rest}`
            if loc.exact_match {
                if let Err(e) = router.insert(path.clone(), proxy_route.clone()) {
                    error!("Failed to register exact route '{}': {}", path, e);
                    continue;
                }
            } else {
                // ПРЕФІКСНЕ МАРШРУТИЗУВАННЯ (Напр. /api => /api/ та /api/*)

                // 1. Точний збіг шляху
                if let Err(e) = router.insert(path.clone(), proxy_route.clone()) {
                    error!("Failed to register exact route '{}': {}", path, e);
                    continue;
                }

                // 2. Wildcard маршрут для перехоплення всіх внутрішніх шляхів
                let catch_all_path = if path == "/" {
                    "/{*rest}".to_string()
                } else if path.ends_with('/') {
                    format!("{}{{*rest}}", path)
                } else {
                    format!("{}/{{*rest}}", path)
                };

                if let Err(e) = router.insert(catch_all_path.clone(), proxy_route) {
                    error!("Failed to register sub-route '{catch_all_path}': {e}");
                    continue;
                }
            }

            info!(
                "✅ Route: '{}' -> {:?} (SNI: {}, Strip: {})",
                loc.path, loc.upstreams, sni_host, loc.strip_prefix
            );
            route_count += 1;
        }
    }

    if route_count == 0 {
        error!("No valid routes configured!");
        return None;
    }

    Some(ProxyRouter {
        routes: router,
        health_checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Location, Route, ServerConfig, Timeouts};

    fn mock_config() -> Config {
        Config {
            server: ServerConfig {
                listen_addr: "0.0.0.0".to_string(),
                listen_port: 8080,
                log_level: "info".to_string(),
                timeouts: Timeouts::default(),
                client_read_timeout: None,
                max_header_size: None,
                global_connections: None,
                prometheus_port: None,
            },
            tls_port: None,
            routes: vec![Route {
                host: "example.com".to_string(),
                tls: None,
                locations: vec![
                    Location {
                        path: "/api".to_string(),
                        upstreams: vec!["127.0.0.1:8081".to_string()],
                        websocket: false,
                        strip_prefix: true,
                        exact_match: false,
                        health_check_path: None,
                        retry_count: 0,
                        max_inflight: None,
                        upstream_tls: None,
                        timeouts: None,
                        client_max_body_size: None,
                        host_header: None,
                    },
                    Location {
                        path: "/exact".to_string(),
                        upstreams: vec!["127.0.0.1:8082".to_string()],
                        websocket: false,
                        strip_prefix: false,
                        exact_match: true,
                        health_check_path: None,
                        retry_count: 0,
                        max_inflight: None,
                        upstream_tls: None,
                        timeouts: None,
                        client_max_body_size: None,
                        host_header: None,
                    },
                ],
            }],
        }
    }

    #[test]
    fn test_build_router_valid() {
        let conf = mock_config();
        let router = build_router(&conf).expect("Expected valid router");

        // Check wildcard matching for prefix path
        let match1 = router.routes.at("/api/users");
        assert!(match1.is_ok());
        let route1 = match1.unwrap().value;
        assert_eq!(route1.path, "/api");
        assert_eq!(route1.strip_prefix, true);

        // Check exact match only matches precisely
        let match2 = router.routes.at("/exact");
        assert!(match2.is_ok());

        let match3 = router.routes.at("/exact/extra");
        assert!(match3.is_err()); // because exact_match = true
    }

    #[test]
    fn test_build_router_empty_upstreams_skipped() {
        let mut conf = mock_config();
        conf.routes[0].locations[0].upstreams = vec![];

        let router = build_router(&conf).expect("Expected valid router with 1 route");

        // Exact route still exists
        assert!(router.routes.at("/exact").is_ok());
        // API route should be skipped
        assert!(router.routes.at("/api").is_err());
    }
}
