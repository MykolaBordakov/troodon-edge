#![allow(dead_code)]
use serde::Deserialize;
use std::collections::HashMap;

// #[derive(Debug, Deserialize)] -> Це макроси.
// Debug - аналог Stringer в Go (дозволяє робити println!("{:?}", conf)).
// Deserialize - генерує код для розпарсингу YAML/JSON у цю структуру (автоматична імплементація інтерфейсу).

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    // Option<T> - це як вказівник (*T) в Go, який може бути nil.
    // Якщо поля немає в YAML, буде None.
    pub tls: Option<TlsConfig>, 
    pub routes: Vec<Route>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    // В Go ти б писав `yaml:"listen_addr"`.
    // Serde вміє це автоматично, якщо імена збігаються, але ми можемо форсувати.
    pub listen_addr: String,
    #[serde(default = "default_port")]
    pub listen_port: u16,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub timeouts: Timeouts,
}

fn default_log_level() -> String {
    "info".to_string()
}
fn default_port() -> u16 {
    6188
}
// Часи в секундах, які ми будемо використовувати для налаштування таймаутів у проксі.

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Timeouts {
    pub connect: u64,
    pub read: u64,
    pub write: u64,
    pub idle: u64,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            connect: 5,
            read: 10,
            write: 10,
            idle: 30,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TlsConfig {
    // HashMap - це map[string]CertificateConfig
    pub certificates: HashMap<String, CertificateConfig>,
}

#[derive(Debug, Deserialize)]
pub struct CertificateConfig {
    pub cert: String,
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct Route {
    pub host: Option<String>,
    pub locations: Vec<Location>,
}

#[derive(Debug, Deserialize)]
pub struct Location {
    pub path: String,
    pub upstreams: Vec<String>,
    
    // default - якщо поля немає, візьми false (значення за замовчуванням для bool).
    #[serde(default)] 
    pub websocket: bool,
    
    #[serde(default)]
    pub strip_prefix: bool,
    
    pub settings: Option<LocationSettings>,
}

#[derive(Debug, Deserialize)]
pub struct LocationSettings {
    pub client_max_body_size: Option<usize>,
}

// Функція-конструктор (Factory pattern).
// Result<Config> - це як (Config, error) в Go.
pub fn load_config(path: &str) -> Result<Config, anyhow::Error> {
    // ? в кінці - це магія Rust. 
    // Це аналог: if err != nil { return nil, err }
    
    let f = std::fs::File::open(path)?; 
    let config: Config = serde_yaml::from_reader(f)?;
    
    Ok(config)
}
