mod background;
mod config;
mod proxy;
mod router;

// Імпортуємо нову структуру ProxyRoute
use arc_swap::ArcSwap;
use pingora::services::listening::Service;
// use matchit::Router;
// use pingora::lb::LoadBalancer;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use pingora::services::background::background_service;
use proxy::LB;
use router::build_router;
use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info, warn};

fn main() {
    // 1. ЗАВАНТАЖЕННЯ
    let conf = match config::load_config("config.yaml") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("🔥 Fatal error loading config: {}", e);
            std::process::exit(1);
        }
    };

    // 2. ЛОГУВАННЯ
    let env_filter = tracing_subscriber::EnvFilter::new(&conf.server.log_level);
    tracing_subscriber::fmt()
        .json() // JSON формат для продакшена/парсингу
        .with_env_filter(env_filter)
        .init();

    info!("Logger initialized with level: '{}'", conf.server.log_level);
    info!("🦖 Troodon is starting...");

    let opt = pingora::server::configuration::Opt::default();
    let mut troodon_server = Server::new(Some(opt)).unwrap();

    // #13: Налаштовуємо Graceful Shutdown drain period.
    // При SIGTERM Pingora: 1) зупиняє прийом нових з'єднань
    // 2) чекає grace_period щоб in-flight запити завершились
    // 3) після graceful_shutdown_timeout — kills Примусово закриває все
    if let Some(server_conf) = Arc::get_mut(&mut troodon_server.configuration) {
        server_conf.grace_period_seconds = Some(30);
        server_conf.graceful_shutdown_timeout_seconds = Some(60);
    }

    // Налаштовуємо глобальні ліміти
    if let Some(max_conn) = conf.server.global_connections {
        // global_connections тепер enforce'ується через Inflight в request_filter.
        // ulimit -n має бути >= max_conn для нормальної роботи.
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
    // Pingora створює свій власний Tokio runtime під капотом,
    // тому використовуємо std::thread і створюємо окремий маленький runtime для фонової задачі
    let hot_reload_router = shared_router.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut sig = signal(SignalKind::hangup()).expect("Failed to bind SIGHUP");
            loop {
                sig.recv().await;
                info!("🔄 Received SIGHUP! Reloading config...");

                match config::load_config("config.yaml") {
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

    let bind_addr = format!(
        "{}:{}",
        server_config.listen_addr, server_config.listen_port
    );
    info!("Troodon is binding to TCP (HTTP): {}", bind_addr);

    lb_service.add_tcp(&bind_addr);

    // Додаємо TLS (HTTPS) слухача, якщо конфігурація присутня
    if let (Some(tls_port), Some(tls_config)) = (server_config.tls_port, &conf.tls) {
        if let Some((_, cert_cfg)) = tls_config.certificates.iter().next() {
            let tls_bind_addr = format!("{}:{}", server_config.listen_addr, tls_port);

            // Pingora має вбудований метод add_tls для простих сертифікатів
            if let Err(e) = lb_service.add_tls(&tls_bind_addr, &cert_cfg.cert, &cert_cfg.key) {
                error!("❌ Failed to bind TLS on {}: {}", tls_bind_addr, e);
            } else {
                info!("🔒 Troodon is binding to TLS (HTTPS): {}", tls_bind_addr);
            }
        } else {
            warn!("⚠️ TLS config present but no certificates found.");
        }
    }

    troodon_server.add_service(lb_service);

    // Включаємо Prometheus, якщо вказаний порт
    if let Some(prom_port) = server_config.prometheus_port {
        let mut prom_service = Service::prometheus_http_service();
        let prom_addr = format!("{}:{}", server_config.listen_addr, prom_port);
        prom_service.add_tcp(&prom_addr);
        troodon_server.add_service(prom_service);
        info!("📊 Prometheus metrics exposed on TCP: {}", prom_addr);
    }

    // Оголошуємо та реєструємо нативний фоновий сервіс Health Check
    let hc_service = background_service(
        "router_health_check",
        background::RouterHealthCheck {
            router: shared_router.clone(),
        },
    );
    troodon_server.add_service(hc_service);

    troodon_server.run_forever();
}
