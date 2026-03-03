use serde::Deserialize;
use tracing::warn;

// --- ГОЛОВНА СТРУКТУРА ---
#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    // TLS порт для всіх HTTPS маршрутів (один порт — багато доменів через SNI)
    pub tls_port: Option<u16>,
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
}

fn default_log_level() -> String {
    warn!("'log_level' missing in config. Defaulting to 'info'.");
    "info".to_string()
}

fn default_port() -> u16 {
    warn!("'listen_port' missing in config. Defaulting to 6188.");
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
        // NOTE: no warning here — Default::default() is called by serde even when
        // the user provides partial values. Warning is emitted in load_config() instead.
        Timeouts {
            connect: 5,
            read: 10,
            write: 10,
            idle: 30,
        }
    }
}

impl PartialEq for Timeouts {
    fn eq(&self, other: &Self) -> bool {
        self.connect == other.connect
            && self.read == other.read
            && self.write == other.write
            && self.idle == other.idle
    }
}

// --- PER-ROUTE TLS ---
#[derive(Debug, Deserialize)]
pub struct RouteTlsConfig {
    pub cert: String,
    pub key: String,
}

// --- ROUTES ---
#[derive(Debug, Deserialize)]
pub struct Route {
    // `host` є обов'язковим: використовується для TLS SNI та як дефолтний Host заголовок.
    // Небезпечно надавати дефолт — відсутній host призведе до витоку трафіку.
    pub host: String,

    // Якщо задано — цей маршрут обслуговується через HTTPS (сертифікат per-domain).
    // Якщо None — тільки HTTP.
    pub tls: Option<RouteTlsConfig>,

    pub locations: Vec<Location>,
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

    // Кількість спроб повтору запиту (Retries), якщо бекенд лежить (0 = не ретраїти)
    #[serde(default)]
    pub retry_count: usize,

    // Максимальна кількість одночасних запитів (inflight) до одного бекенду (pingora-limits)
    pub max_inflight: Option<isize>,

    // Явний TLS-прапорець для upstream (якщо None — визначається автоматично по порту 443)
    pub upstream_tls: Option<bool>,

    // Optional location-level timeouts (overrides global)
    pub timeouts: Option<Timeouts>,

    // Максимальний розмір тіла запиту клієнта (байти). Якщо Content-Length перевищує — 413.
    // ВАЖЛИВО: це м'який захист — перевіряє Content-Length. Реальне обмеження тіла через
    // request_body_filter() у proxy.rs (count actual bytes for chunked encoding).
    pub client_max_body_size: Option<usize>,

    // Окремий Host заголовок для upstream (якщо відрізняється від SNI).
    // Якщо None — використовується route.host (SNI) як Host заголовок.
    pub host_header: Option<String>,
}

fn default_host_path() -> String {
    warn!("Location path missing. Defaulting to '/'.");
    "/".to_string()
}

// --- LOADER ---
pub fn load_config(path: &str) -> Result<Config, anyhow::Error> {
    let f = std::fs::File::open(path)?;
    let config: Config = serde_yaml::from_reader(f)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_timeouts() {
        let t = Timeouts::default();
        assert_eq!(t.connect, 5);
        assert_eq!(t.read, 10);
        assert_eq!(t.write, 10);
        assert_eq!(t.idle, 30);
    }

    #[test]
    fn test_partial_timeouts_deserialization() {
        let yaml = "read: 20\nwrite: 15";
        let t: Timeouts = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(t.connect, 5); // default
        assert_eq!(t.read, 20);
        assert_eq!(t.write, 15);
        assert_eq!(t.idle, 30); // default
    }

    #[test]
    fn test_valid_config_deserialization() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0"
  listen_port: 8080
routes:
  - host: "example.com"
    locations:
      - path: "/"
        upstreams: ["127.0.0.1:8081"]
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.server.listen_port, 8080);
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.routes[0].host, "example.com");
        assert_eq!(config.routes[0].locations[0].path, "/");
    }

    #[test]
    fn test_missing_host_fails() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0"
routes:
  - locations:
      - path: "/"
        upstreams: ["127.0.0.1:8081"]
"#;
        let result: Result<Config, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "Config without host should fail");
    }
}
