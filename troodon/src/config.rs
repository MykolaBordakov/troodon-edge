#![allow(dead_code)] // Щоб Rust не сварився на поля, які ми поки не юзаємо (напр. tls)

use serde::Deserialize;
use std::collections::HashMap;

// --- ГОЛОВНА СТРУКТУРА ---
#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub tls: Option<TlsConfig>,
    pub routes: Vec<Route>,
}

// --- SERVER CONFIG ---
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: String,

    #[serde(default = "default_port")]
    pub listen_port: u16,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default)] // Викличе Timeouts::default()
    pub timeouts: Timeouts,

    // L7 Security (Slowloris & OOM Protection)
    pub client_read_timeout: Option<u64>,
    pub max_header_size: Option<usize>,
    pub global_connections: Option<usize>,

    // Порт для експорту метрик Prometheus (опціонально)
    pub prometheus_port: Option<u16>,

    // Порт для HTTPS/TLS трафіку (опціонально)
    pub tls_port: Option<u16>,
}

fn default_log_level() -> String {
    // Використовуємо eprintln!, щоб додати новий рядок
    eprintln!("⚠️ [WARN] 'log_level' missing in config. Defaulting to 'info'.");
    "info".to_string()
}

fn default_port() -> u16 {
    eprintln!("⚠️ [WARN] 'listen_port' missing in config. Defaulting to 6188.");
    6188
}

// --- TIMEOUTS ---
#[derive(Debug, Deserialize, Clone)]
#[serde(default)] // Дозволяє часткове заповнення (наприклад, тільки read)
pub struct Timeouts {
    pub connect: u64,
    pub read: u64,
    pub write: u64,
    pub idle: u64,
}

impl Default for Timeouts {
    fn default() -> Self {
        // Форматуємо гарно, щоб не ламати консоль
        eprintln!(
            "⚠️ [WARN] Timeouts not fully specified. Using defaults: connect=5s, read=10s, write=10s, idle=30s."
        );
        Timeouts {
            connect: 5,
            read: 10,
            write: 10,
            idle: 30,
        }
    }
}

// --- TLS ---
#[derive(Debug, Deserialize)]
pub struct TlsConfig {
    pub certificates: HashMap<String, CertificateConfig>,
}

#[derive(Debug, Deserialize)]
pub struct CertificateConfig {
    pub cert: String,
    pub key: String,
}

// --- ROUTES ---
#[derive(Debug, Deserialize)]
pub struct Route {
    // Тут ми гарантуємо String. Якщо в YAML немає host — буде дефолт.
    #[serde(default = "default_host")]
    pub host: String,
    pub locations: Vec<Location>,
}

fn default_host() -> String {
    // ВАЖЛИВО: SNI не може бути "*".
    // Для тесту використовуємо one.one.one.one, в реальності тут може бути пустий рядок,
    // який ми обробимо як "не надсилати SNI".
    eprintln!("⚠️ [WARN] Route 'host' missing. Defaulting to 'one.one.one.one' (Cloudflare).");
    "one.one.one.one".to_string()
}

// --- LOCATIONS ---
#[derive(Debug, Deserialize)]
pub struct Location {
    #[serde(default = "default_host_path")]
    pub path: String,

    pub upstreams: Vec<String>,

    #[serde(default)]
    pub websocket: bool,

    #[serde(default)]
    pub strip_prefix: bool,

    // Контролює, чи це точний збіг (наприклад "/api"), чи ми додаємо wildcard ("/*rest")
    #[serde(default)]
    pub exact_match: bool,

    // Шлях для Active Health Check (якщо None, тоді Active Health Check вимкнено)
    pub health_check_path: Option<String>,

    // Кількість спроб повтору запиту (Retries), якщо бекенд лежить
    pub retry_count: Option<usize>,

    // Максимальна кількість одночасних запитів (inflight) до одного бекенду (pingora-limits)
    pub max_inflight: Option<isize>,

    // Optional location-level timeouts (overrides global)
    pub timeouts: Option<Timeouts>,

    pub settings: Option<LocationSettings>,
}

fn default_host_path() -> String {
    eprintln!("⚠️ [WARN] Location path missing. Defaulting to '/'.");
    "/".to_string()
}

#[derive(Debug, Deserialize)]
pub struct LocationSettings {
    pub client_max_body_size: Option<usize>,
}

// --- LOADER ---
pub fn load_config(path: &str) -> Result<Config, anyhow::Error> {
    let f = std::fs::File::open(path)?;
    let config: Config = serde_yaml::from_reader(f)?;
    Ok(config)
}
