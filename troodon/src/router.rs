use crate::config;
use crate::proxy::{ProxyRoute, ProxyRouter};
use crate::security::IpFilter;
use matchit::Router;
use pingora::lb::LoadBalancer;
use pingora::lb::health_check::HttpHealthCheck;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

pub fn build_router(conf: &config::Config) -> Option<ProxyRouter> {
    // routes: hostname → per-host Router (path → ProxyRoute)
    let mut routes: HashMap<String, Router<Arc<ProxyRoute>>> = HashMap::new();
    let mut health_checks = Vec::new();
    let mut route_count = 0;

    for route_conf in &conf.routes {
        let sni_host = route_conf.host.clone();

        // Ініціалізуємо фільтр для маршруту лише один раз
        let route_ip_filter = route_conf
            .ip_access_control
            .as_ref()
            .map(|c| Arc::new(IpFilter::new(c)));

        // Отримуємо або створюємо per-host router
        let host_router = routes.entry(sni_host.clone()).or_default();

        for loc in &route_conf.locations {
            if loc.upstreams.is_empty() {
                error!(
                    "Location '{}' for host '{}' has no upstreams defined! Skipping.",
                    loc.path, sni_host
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
                let hc_tls = loc.upstream_tls.unwrap_or(false);
                let mut hc = HttpHealthCheck::new(&sni_host, hc_tls);
                if let Ok(uri) = hc_path.parse() {
                    hc.req.set_uri(uri);
                }

                // Встановлюємо правильний Host заголовок для Health Check (дуже важливо для деяких бекендів)
                let host_hdr = loc.host_header.as_deref().unwrap_or(&sni_host);
                hc.req.insert_header("Host", host_hdr).unwrap();

                lb.set_health_check(Box::new(hc));
                info!(
                    "🔬 Enabled HTTP Health Check (path: '{}') for Location: {}",
                    hc_path, loc.path
                );
            }

            let lb_arc = Arc::new(lb);

            if loc.health_check_path.is_some() {
                health_checks.push((loc.path.clone(), lb_arc.clone()));
            }

            // Визначаємо Timeouts для локації
            let loc_timeouts = loc
                .timeouts
                .clone()
                .unwrap_or_else(|| conf.server.timeouts.clone());

            let proxy_route = Arc::new(ProxyRoute {
                path: loc.path.as_str().into(),
                lb: lb_arc.clone(),
                sni: sni_host.as_str().into(),
                host_header: loc.host_header.as_deref().map(Into::into),
                health_check_path: loc.health_check_path.as_deref().map(Into::into),
                rate_limit: loc.req_per_sec.map(|r| {
                    (
                        Arc::new(pingora_limits::rate::Rate::new(
                            std::time::Duration::from_secs(1),
                        )),
                        r,
                    )
                }),
                strip_prefix: loc.strip_prefix,
                max_inflight: loc.max_inflight,
                timeouts: loc_timeouts,
                retry_count: loc.retry_count,
                upstream_tls: loc.upstream_tls,
                websocket: loc.websocket,
                client_max_body_size: loc.client_max_body_size,
                upstream_http2: loc.upstream_http2,
                ip_filter: route_ip_filter.clone(),
            });

            let path = loc.path.clone();

            if loc.exact_match {
                if let Err(e) = host_router.insert(path.clone(), proxy_route.clone()) {
                    error!(
                        "Failed to register exact route '{}' for host '{}': {}",
                        path, sni_host, e
                    );
                    continue;
                }
            } else {
                // ПРЕФІКСНЕ МАРШРУТИЗУВАННЯ

                // 1. Точний збіг шляху
                if let Err(e) = host_router.insert(path.clone(), proxy_route.clone()) {
                    error!(
                        "Failed to register route '{}' for host '{}': {}",
                        path, sni_host, e
                    );
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

                if let Err(e) = host_router.insert(catch_all_path.clone(), proxy_route) {
                    error!(
                        "Failed to register sub-route '{}' for host '{}': {}",
                        catch_all_path, sni_host, e
                    );
                    continue;
                }
            }

            info!(
                "✅ Route: '{}' @ host '{}' -> {:?} (Strip: {})",
                loc.path, sni_host, loc.upstreams, loc.strip_prefix
            );
            route_count += 1;
        }
    }

    if route_count == 0 {
        error!("No valid routes configured!");
        return None;
    }

    Some(ProxyRouter {
        routes,
        health_checks,
    })
}
