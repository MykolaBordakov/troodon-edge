mod config;
mod proxy;

// Імпортуємо нову структуру ProxyRoute
use proxy::{LB, ProxyRoute}; 
use pingora::prelude::*;
use std::sync::Arc;
use pingora::lb::LoadBalancer;
use pingora::proxy::http_proxy_service;
use tracing::{info, error};

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
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    info!("Logger initialized with level: '{}'", conf.server.log_level);
    info!("🦖 Troodon is starting...");

    let mut troodon_server = Server::new(None).unwrap();
    troodon_server.bootstrap();

    // 3. ПІДГОТОВКА МАРШРУТІВ (ROUTING ENGINE)
    // Ми проходимо по всіх Routes -> Locations і створюємо плоский список ProxyRoute
    
    let mut active_routes: Vec<ProxyRoute> = Vec::new();

    for route_conf in &conf.routes {
        // config.rs гарантує, що тут є рядок (наприклад, "one.one.one.one")
        let sni_host = route_conf.host.clone();

        for loc in &route_conf.locations {
            if loc.upstreams.is_empty() {
                error!("Location '{}' has no upstreams defined! Exiting.", loc.path);
                std::process::exit(1);
            }

            // Створюємо балансувальник для конкретної локації
            let lb = LoadBalancer::try_from_iter(&loc.upstreams)
                .expect("Failed to initialize Load Balancer (check IPs format)");

            info!(
                "✅ Registered route: Path='{}' -> Upstreams={:?} (SNI: {})", 
                loc.path, loc.upstreams, sni_host
            );

            // Додаємо в список
            active_routes.push(ProxyRoute {
                path: loc.path.clone(),
                lb: Arc::new(lb),
                sni: sni_host.clone(),
            });
        }
    }

    if active_routes.is_empty() {
        error!("No routes configured! Please check your config.yaml");
        std::process::exit(1);
    }

    // 4. ІНІЦІАЛІЗАЦІЯ СЕРВІСУ
    let server_config = Arc::new(conf.server);

    let lb_instance = LB {
        // Загортаємо весь список маршрутів в Arc
        routes: Arc::new(active_routes), 
        config: server_config.clone(),
    };

    let mut lb_service = http_proxy_service(&troodon_server.configuration, lb_instance);
    
    let bind_addr = format!("{}:{}", server_config.listen_addr, server_config.listen_port);
    info!("Troodon is binding to TCP: {}", bind_addr);
    
    lb_service.add_tcp(&bind_addr);
    troodon_server.add_service(lb_service);
    troodon_server.run_forever();
}