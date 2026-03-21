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

    // Глобальна IP-фільтрація
    pub ip_access_control: Option<IpAccessControl>,

    // Порт для експорту метрик Prometheus (опціонально)
    pub prometheus_port: Option<u16>,

    #[serde(default = "default_prom_addr")]
    pub prometheus_listen_addr: String,
}

fn default_prom_addr() -> String {
    "127.0.0.1".to_string()
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

// --- IP ACCESS CONTROL ---
#[derive(Debug, Deserialize, Clone)]
pub struct IpAccessControl {
    pub enabled: bool,
    pub default_action: String, // "allow" або "deny"
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub blacklist: Vec<String>,
}

// --- MTLS CONFIG ---
#[derive(Debug, Deserialize, Clone)]
pub struct MtlsConfig {
    pub enabled: bool,
    pub client_ca: Option<String>,
}

// --- PER-ROUTE TLS ---
#[derive(Debug, Deserialize)]
pub struct RouteTlsConfig {
    pub cert: String,
    pub key: String,
    // Вмикає або вимикає HTTP/2 на вхід (додає ALPN h2 в сертифікат)
    #[serde(default = "default_true")]
    pub http2: bool,
    // Налаштування mTLS для цього маршруту
    pub mtls: Option<MtlsConfig>,
}

fn default_true() -> bool {
    true
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

    // Локальна IP-фільтрація (per-route)
    pub ip_access_control: Option<IpAccessControl>,

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
    // Rate limit per IP (requests per second)
    pub req_per_sec: Option<isize>,

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

    // Чи використовувати HTTP/2 для підключення до бекенду
    #[serde(default)]
    pub upstream_http2: bool,
}

fn default_host_path() -> String {
    warn!("Location path missing. Defaulting to '/'.");
    "/".to_string()
}

// --- LOADER + VALIDATOR ---

/// Завантажує і валідує конфігурацію.
/// Помилки валідації — зрозумілі повідомлення з конкретним полем.
pub fn load_config(path: &str) -> Result<Config, anyhow::Error> {
    let f = std::fs::File::open(path)?;
    let config: Config = serde_yml::from_reader(f)?;
    validate(&config)?;
    Ok(config)
}

/// Семантична валідація конфігурації.
/// Викликається при старті та при hot reload.
pub fn validate(config: &Config) -> Result<(), anyhow::Error> {
    // 1. listen_addr має бути валідною IP-адресою
    config
        .server
        .listen_addr
        .parse::<std::net::IpAddr>()
        .map_err(|_| {
            anyhow::anyhow!(
                "Invalid listen_addr '{}': must be a valid IP address (e.g. '0.0.0.0' or '127.0.0.1')",
                config.server.listen_addr
            )
        })?;

    // 2. log_level має бути одним з допустимих значень
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&config.server.log_level.to_lowercase().as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid log_level '{}': must be one of {:?}",
            config.server.log_level,
            valid_levels
        ));
    }

    // 3. Всі timeout поля мають бути > 0
    let t = &config.server.timeouts;
    if t.connect == 0 || t.read == 0 || t.write == 0 || t.idle == 0 {
        return Err(anyhow::anyhow!(
            "All server timeouts must be > 0. Got: connect={}, read={}, write={}, idle={}",
            t.connect, t.read, t.write, t.idle
        ));
    }

    // 4. Перевірка конфліктів портів
    let listen_port = config.server.listen_port;
    if let Some(tls_port) = config.tls_port {
        if tls_port == listen_port {
            return Err(anyhow::anyhow!(
                "Port conflict: tls_port ({}) cannot equal listen_port ({})",
                tls_port, listen_port
            ));
        }
        if let Some(prom_port) = config.server.prometheus_port {
            if prom_port == tls_port {
                return Err(anyhow::anyhow!(
                    "Port conflict: prometheus_port ({}) cannot equal tls_port ({})",
                    prom_port, tls_port
                ));
            }
        }
    }
    if let Some(prom_port) = config.server.prometheus_port {
        if prom_port == listen_port {
            return Err(anyhow::anyhow!(
                "Port conflict: prometheus_port ({}) cannot equal listen_port ({})",
                prom_port, listen_port
            ));
        }
    }

    // 5. Перевірка ip_access_control default_action
    if let Some(ref iac) = config.server.ip_access_control {
        let action = iac.default_action.to_lowercase();
        if action != "allow" && action != "deny" {
            return Err(anyhow::anyhow!(
                "Invalid ip_access_control.default_action '{}': must be 'allow' or 'deny'",
                iac.default_action
            ));
        }
    }

    // 6. Перевіряємо маршрути
    if config.routes.is_empty() {
        return Err(anyhow::anyhow!("Configuration must define at least one route"));
    }

    for route in &config.routes {
        // 6a. per-route ip_access_control default_action
        if let Some(ref iac) = route.ip_access_control {
            let action = iac.default_action.to_lowercase();
            if action != "allow" && action != "deny" {
                return Err(anyhow::anyhow!(
                    "Invalid ip_access_control.default_action '{}' for route host='{}': must be 'allow' or 'deny'",
                    iac.default_action, route.host
                ));
            }
        }

        // 6b. TLS cert/key файли мають існувати
        if let Some(ref tls) = route.tls {
            if !std::path::Path::new(&tls.cert).exists() {
                return Err(anyhow::anyhow!(
                    "TLS cert file not found for host '{}': '{}'",
                    route.host, tls.cert
                ));
            }
            if !std::path::Path::new(&tls.key).exists() {
                return Err(anyhow::anyhow!(
                    "TLS key file not found for host '{}': '{}'",
                    route.host, tls.key
                ));
            }
            // 6c. Якщо mTLS enabled — client_ca теж має існувати
            if let Some(ref mtls) = tls.mtls {
                if mtls.enabled {
                    let ca = mtls.client_ca.as_ref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "mTLS is enabled for host '{}' but client_ca is not set",
                            route.host
                        )
                    })?;
                    if !std::path::Path::new(ca).exists() {
                        return Err(anyhow::anyhow!(
                            "mTLS client_ca file not found for host '{}': '{}'",
                            route.host, ca
                        ));
                    }
                }
            }
        }

        // 6d. location timeouts
        for loc in &route.locations {
            if let Some(ref lt) = loc.timeouts {
                if lt.connect == 0 || lt.read == 0 || lt.write == 0 || lt.idle == 0 {
                    return Err(anyhow::anyhow!(
                        "All timeouts in location '{}' (host='{}') must be > 0",
                        loc.path, route.host
                    ));
                }
            }
        }
    }

    Ok(())
}
