mod background;
mod config;
mod proxy;
mod router;

use arc_swap::ArcSwap;
use openssl::ssl::{NameType, SslContextBuilder, SslFiletype, SslMethod};
use pingora::listeners::tls::TlsSettings;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use pingora::services::background::background_service;
use pingora::services::listening::Service;
use proxy::LB;
use router::build_router;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info, warn};

fn main() {
    let config_path = std::env::var("TROODON_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());
    let conf = match config::load_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("🔥 Fatal error loading config {}: {}", config_path, e);
            std::process::exit(1);
        }
    };

    // 2. ЛОГУВАННЯ
    let env_filter = tracing_subscriber::EnvFilter::new(&conf.server.log_level);
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(env_filter)
        .init();

    info!("Logger initialized with level: '{}'", conf.server.log_level);
    info!("🦖 Troodon is starting...");

    let opt = pingora::server::configuration::Opt::default();
    let mut troodon_server =
        Server::new(Some(opt)).expect("Failed to initialize Pingora server. Check configuration.");

    // Налаштовуємо Graceful Shutdown drain period.
    if let Some(server_conf) = Arc::get_mut(&mut troodon_server.configuration) {
        server_conf.grace_period_seconds = Some(30);
        server_conf.graceful_shutdown_timeout_seconds = Some(60);
    }

    // Глобальні ліміти
    if let Some(max_conn) = conf.server.global_connections {
        info!(
            "🔒 Global connection limit set to {}. Enforced via Inflight guard (returns 429 on overflow).",
            max_conn
        );
    }

    troodon_server.bootstrap();

    // 3. ПІДГОТОВКА МАРШРУТІВ (ROUTING ENGINE)
    let proxy_router = match build_router(&conf) {
        Some(r) => r,
        None => std::process::exit(1),
    };

    let shared_router = Arc::new(ArcSwap::from_pointee(proxy_router));
    let server_config = Arc::new(conf.server);

    // 4. ФОНОВИЙ ЗАДАЧА ДЛЯ HOT RELOAD (SIGHUP)
    //
    // ⚠️  ВАЖЛИВО: Hot reload оновлює ТІЛЬКИ таблицю маршрутів (routes).
    // Зміни в `server` секції конфігу (log_level, global_connections, timeouts тощо)
    // НЕ застосовуються без повного перезапуску процесу.
    let hot_reload_router = shared_router.clone();
    let hot_reload_config_path = config_path.clone();
    std::thread::spawn(move || {
        let config_path = hot_reload_config_path;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build hot-reload Tokio runtime");
        rt.block_on(async {
            let mut sig = signal(SignalKind::hangup()).expect("Failed to bind SIGHUP");
            loop {
                sig.recv().await;
                info!("🔄 Received SIGHUP! Reloading config...");
                warn!("⚠️  Hot reload updates routing table ONLY. Server config changes require a full restart.");

                match config::load_config(&config_path) {
                    Ok(new_conf) => {
                        if let Some(new_router) = build_router(&new_conf) {
                            hot_reload_router.store(Arc::new(new_router));
                            info!("✅ Hot reload successful! Routing table updated atomically.");
                        } else {
                            warn!("⚠️ New config has no valid routes. Keeping old routing table.");
                        }
                    }
                    Err(e) => {
                        error!("❌ Failed to parse new config during hot reload: {}", e);
                    }
                }
            }
        });
    });

    // 5. ІНІЦІАЛІЗАЦІЯ СЕРВІСУ
    let lb_instance = LB {
        router: shared_router.clone(),
        config: server_config.clone(),
        inflight: Arc::new(pingora_limits::inflight::Inflight::new()),
    };

    let mut lb_service = http_proxy_service(&troodon_server.configuration, lb_instance);

    // 6. HTTP СЛУХАЧ
    let bind_addr = format!(
        "{}:{}",
        server_config.listen_addr, server_config.listen_port
    );
    info!("Troodon is binding to TCP (HTTP): {}", bind_addr);
    lb_service.add_tcp(&bind_addr);

    // 7. HTTPS / TLS СЛУХАЧ — SNI-based multi-cert
    // Кожен route може мати свій сертифікат. Один порт — багато доменів через SNI.
    if let Some(tls_port) = conf.tls_port {
        let tls_routes: Vec<(String, String, String)> = conf
            .routes
            .iter()
            .filter_map(|r| {
                r.tls
                    .as_ref()
                    .map(|t| (r.host.clone(), t.cert.clone(), t.key.clone()))
            })
            .collect();

        if tls_routes.is_empty() {
            warn!(
                "⚠️ tls_port is set to {} but no routes have TLS configured. Skipping HTTPS listener.",
                tls_port
            );
        } else {
            let tls_addr = format!("{}:{}", server_config.listen_addr, tls_port);
            let (first_host, first_cert, first_key) = &tls_routes[0];

            match TlsSettings::intermediate(first_cert, first_key) {
                Err(e) => {
                    error!(
                        "❌ Failed to load default TLS cert for '{}': {}. HTTPS listener NOT started.",
                        first_host, e
                    );
                }
                Ok(mut tls_settings) => {
                    // Пре-будуємо SslContext для кожного домену через openssl SslContextBuilder.
                    // Уникаємо file I/O під час handshake — всі cert завантажені на старті.
                    let mut sni_map: HashMap<String, Arc<openssl::ssl::SslContext>> =
                        HashMap::new();

                    for (host, cert, key) in &tls_routes {
                        let ctx_result = (|| -> anyhow::Result<openssl::ssl::SslContext> {
                            let mut b = SslContextBuilder::new(SslMethod::tls_server())?;
                            b.set_certificate_chain_file(cert)?;
                            b.set_private_key_file(key, SslFiletype::PEM)?;
                            Ok(b.build())
                        })();

                        match ctx_result {
                            Ok(ctx) => {
                                sni_map.insert(host.clone(), Arc::new(ctx));
                                info!("🔒 TLS cert loaded for domain: {}", host);
                            }
                            Err(e) => {
                                error!("❌ Failed to load TLS cert for domain '{}': {}", host, e);
                            }
                        }
                    }

                    let sni_map = Arc::new(sni_map);

                    // SNI callback: строга перевірка.
                    // Без SNI або невідомий домен → ALERT_FATAL.
                    // Ніяких дефолтних сертифікатів — 2026 рік, IE6 не підтримуємо.
                    tls_settings.set_servername_callback(move |ssl, _| {
                        match ssl.servername(NameType::HOST_NAME) {
                            Some(name) => match sni_map.get(name) {
                                Some(ctx) => {
                                    let _ = ssl.set_ssl_context(ctx);
                                    Ok(())
                                }
                                // Домен є, але не налаштований → відхиляємо
                                None => Err(openssl::ssl::SniError::ALERT_FATAL),
                            },
                            // Немає SNI взагалі → відхиляємо
                            None => Err(openssl::ssl::SniError::ALERT_FATAL),
                        }
                    });

                    lb_service.add_tls_with_settings(&tls_addr, None, tls_settings);
                    info!(
                        "🔒 Troodon is binding to TLS (HTTPS): {} ({} domain(s))",
                        tls_addr,
                        tls_routes.len()
                    );
                }
            }
        }
    }

    troodon_server.add_service(lb_service);

    // 8. PROMETHEUS
    if let Some(prom_port) = server_config.prometheus_port {
        let mut prom_service = Service::prometheus_http_service();
        let prom_addr = format!("{}:{}", server_config.listen_addr, prom_port);
        prom_service.add_tcp(&prom_addr);
        troodon_server.add_service(prom_service);
        info!("📊 Prometheus metrics exposed on TCP: {}", prom_addr);
    }

    // 9. HEALTH CHECK BACKGROUND SERVICE
    let hc_service = background_service(
        "router_health_check",
        background::RouterHealthCheck {
            router: shared_router.clone(),
        },
    );
    troodon_server.add_service(hc_service);

    troodon_server.run_forever();
}
