use crate::config::{Config, Timeouts};

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
