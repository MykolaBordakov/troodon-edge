use crate::config::{Config, IpAccessControl, ServerConfig, Timeouts, validate};

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
    let t: Timeouts = serde_yml::from_str(yaml).unwrap();
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
    let config: Config = serde_yml::from_str(yaml).unwrap();
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
    let result: Result<Config, _> = serde_yml::from_str(yaml);
    assert!(result.is_err(), "Config without host should fail");
}

// --- validate() tests ---

fn minimal_valid_config() -> Config {
    use crate::config::Location;
    use crate::config::Route;
    Config {
        server: ServerConfig {
            listen_addr: "0.0.0.0".to_string(),
            listen_port: 8080,
            log_level: "info".to_string(),
            timeouts: Timeouts::default(),
            client_read_timeout: None,
            max_header_size: None,
            global_connections: None,
            ip_access_control: None,
            prometheus_port: None,
            prometheus_listen_addr: "127.0.0.1".to_string(),
        },
        tls_port: None,
        routes: vec![Route {
            host: "example.com".to_string(),
            tls: None,
            ip_access_control: None,
            locations: vec![Location {
                path: "/".to_string(),
                upstreams: vec!["127.0.0.1:8081".to_string()],
                websocket: false,
                strip_prefix: false,
                exact_match: false,
                health_check_path: None,
                req_per_sec: None,
                retry_count: 0,
                max_inflight: None,
                upstream_tls: None,
                timeouts: None,
                client_max_body_size: None,
                host_header: None,
                upstream_http2: false,
            }],
        }],
    }
}

#[test]
fn test_validate_valid_config() {
    let conf = minimal_valid_config();
    assert!(validate(&conf).is_ok());
}

#[test]
fn test_validate_invalid_listen_addr() {
    let mut conf = minimal_valid_config();
    conf.server.listen_addr = "not-an-ip".to_string();
    let err = validate(&conf).unwrap_err();
    assert!(err.to_string().contains("listen_addr"), "Error: {}", err);
}

#[test]
fn test_validate_invalid_log_level() {
    let mut conf = minimal_valid_config();
    conf.server.log_level = "banana".to_string();
    let err = validate(&conf).unwrap_err();
    assert!(err.to_string().contains("log_level"), "Error: {}", err);
}

#[test]
fn test_validate_zero_timeout() {
    let mut conf = minimal_valid_config();
    conf.server.timeouts.connect = 0;
    let err = validate(&conf).unwrap_err();
    assert!(err.to_string().contains("timeout"), "Error: {}", err);
}

#[test]
fn test_validate_port_conflict_listen_tls() {
    let mut conf = minimal_valid_config();
    conf.tls_port = Some(8080); // same as listen_port
    let err = validate(&conf).unwrap_err();
    assert!(err.to_string().contains("conflict"), "Error: {}", err);
}

#[test]
fn test_validate_port_conflict_prom_listen() {
    let mut conf = minimal_valid_config();
    conf.server.prometheus_port = Some(8080); // same as listen_port
    let err = validate(&conf).unwrap_err();
    assert!(err.to_string().contains("conflict"), "Error: {}", err);
}

#[test]
fn test_validate_invalid_default_action() {
    let mut conf = minimal_valid_config();
    conf.server.ip_access_control = Some(IpAccessControl {
        enabled: true,
        default_action: "banana".to_string(),
        whitelist: vec![],
        blacklist: vec![],
    });
    let err = validate(&conf).unwrap_err();
    assert!(err.to_string().contains("default_action"), "Error: {}", err);
}

#[test]
fn test_validate_empty_routes() {
    let mut conf = minimal_valid_config();
    conf.routes.clear();
    let err = validate(&conf).unwrap_err();
    assert!(err.to_string().contains("route"), "Error: {}", err);
}
