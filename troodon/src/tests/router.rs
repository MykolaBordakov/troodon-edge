use crate::config::{Config, Location, Route, ServerConfig, Timeouts};
use crate::router::build_router;

fn mock_config() -> Config {
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
            locations: vec![
                Location {
                    path: "/api".to_string(),
                    upstreams: vec!["127.0.0.1:8081".to_string()],
                    websocket: false,
                    strip_prefix: true,
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
                },
                Location {
                    path: "/exact".to_string(),
                    upstreams: vec!["127.0.0.1:8082".to_string()],
                    websocket: false,
                    strip_prefix: false,
                    exact_match: true,
                    health_check_path: None,
                    req_per_sec: None,
                    retry_count: 0,
                    max_inflight: None,
                    upstream_tls: None,
                    timeouts: None,
                    client_max_body_size: None,
                    host_header: None,
                    upstream_http2: false,
                },
            ],
        }],
    }
}

#[test]
fn test_build_router_valid() {
    let conf = mock_config();
    let router = build_router(&conf).expect("Expected valid router");

    let host_router = router
        .routes
        .get("example.com")
        .expect("Should have router for example.com");

    // Check wildcard matching for prefix path
    let match1 = host_router.at("/api/users");
    assert!(match1.is_ok());
    let route1 = match1.unwrap().value;
    assert_eq!(route1.path.as_ref(), "/api");
    assert_eq!(route1.strip_prefix, true);

    // Check exact match only matches precisely
    let match2 = host_router.at("/exact");
    assert!(match2.is_ok());

    let match3 = host_router.at("/exact/extra");
    assert!(match3.is_err()); // because exact_match = true
}

#[test]
fn test_build_router_empty_upstreams_skipped() {
    let mut conf = mock_config();
    conf.routes[0].locations[0].upstreams = vec![];

    let router = build_router(&conf).expect("Expected valid router with 1 route");

    let host_router = router
        .routes
        .get("example.com")
        .expect("Should have router for example.com");

    // Exact route still exists
    assert!(host_router.at("/exact").is_ok());
    // API route should be skipped
    assert!(host_router.at("/api").is_err());
}

#[test]
fn test_host_isolation() {
    // Two different hosts should get separate routers
    let conf = Config {
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
        routes: vec![
            Route {
                host: "api.example.com".to_string(),
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
            },
            Route {
                host: "admin.example.com".to_string(),
                tls: None,
                ip_access_control: None,
                locations: vec![Location {
                    path: "/".to_string(),
                    upstreams: vec!["127.0.0.1:9090".to_string()],
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
            },
        ],
    };

    let router = build_router(&conf).expect("Expected valid router");

    // Both hosts exist in the map
    assert!(router.routes.contains_key("api.example.com"));
    assert!(router.routes.contains_key("admin.example.com"));

    // Unknown host is not present — would result in 404 at runtime
    assert!(!router.routes.contains_key("evil.example.com"));
}

#[test]
fn test_unknown_host_returns_no_entry() {
    let conf = mock_config();
    let router = build_router(&conf).expect("Expected valid router");

    // Hosts not in config → no entry in HashMap → 404 at runtime
    assert!(router.routes.get("evil.com").is_none());
    assert!(router.routes.get("").is_none());
}
