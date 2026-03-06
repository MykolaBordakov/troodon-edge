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
                upstream_http2: loc.upstream_http2,
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
