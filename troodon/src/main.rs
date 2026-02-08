mod proxy;
use proxy::LB;
mod config;
use pingora::prelude::*;
use std::sync::Arc;
//use pingora::proxy::{http_proxy_service, ProxyHttp, Session};
use pingora::lb::LoadBalancer;
use tracing::{info, error, debug};

// Наша структура, яка тримає балансувальник


// ВАЖЛИВО: Додаємо tokio::main, щоб запустити асинхронний світ
//#[tokio::main]
fn main() {
    println!("🦖 Troodon is reading configuration...");
    let conf = match config::load_config("config.yaml") {
        Ok(c) => c,
        Err(e) => {
            // Якщо конфіг битий — ми навіть не стартуємо.
            eprintln!("🔥 Fatal error loading config: {}", e);
            std::process::exit(1);
        }
    };
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&conf.server.log_level));

    // Ініціалізуємо "Subscriber", який буде писати в консоль (fmt)
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();
    
    info!("Logger initialized with level: '{}'", conf.server.log_level);
    info!("🦖 Troodon is starting...");
    let mut troodon_server = Server::new(None).unwrap();
    troodon_server.bootstrap();

    // 4. ОТРИМАННЯ АПСТРІМІВ (BACKENDS)
    // Шукаємо в конфізі location з шляхом "/"
    // Це Rust-way роботи з колекціями (Iterator API)
    let upstreams_list = conf.routes.iter()
        .flat_map(|r| &r.locations)
        .find(|l| l.path == "/")
        .map(|l| &l.upstreams)
        .expect("CRITICAL: Config must have a default '/' route!");

    if upstreams_list.is_empty() {
        error!("No upstreams defined in config!");
        std::process::exit(1);
    }
    
    debug!("Loaded upstreams for root: {:?}", upstreams_list);

    // Створюємо Load Balancer з даних YAML
    let upstreams = LoadBalancer::try_from_iter(upstreams_list).unwrap();
    let server_config = Arc::new(conf.server);

    // Створюємо LB вже як нормальну структуру
    let lb_instance = LB {
        upstreams: Arc::new(upstreams),
        config: server_config.clone(), // Передаємо конфіг всередину
    };

    // 5. ЗАПУСК СЕРВІСУ
    let mut lb_service = http_proxy_service(
        &troodon_server.configuration, lb_instance);
    
    // Склеюємо IP та Port: "0.0.0.0" + ":" + "6188"
    let bind_addr = format!("{}:{}", server_config.listen_addr, server_config.listen_port);
    
    info!("Troodon is binding to TCP: {}", bind_addr);
    lb_service.add_tcp(&bind_addr);

    troodon_server.add_service(lb_service);
    troodon_server.run_forever();
}