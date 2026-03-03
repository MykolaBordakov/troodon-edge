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
            prometheus_port: None,
        },
        tls_port: None,
        routes: vec![Route {
            host: "example.com".to_string(),
            tls: None,
            locations: vec![
                Location {
                    path: "/api".to_string(),
                    upstreams: vec!["127.0.0.1:8081".to_string()],
                    websocket: false,
                    strip_prefix: true,
                    exact_match: false,
                    health_check_path: None,
                    retry_count: 0,
                    max_inflight: None,
                    upstream_tls: None,
                    timeouts: None,
                    client_max_body_size: None,
                    host_header: None,
                },
                Location {
                    path: "/exact".to_string(),
                    upstreams: vec!["127.0.0.1:8082".to_string()],
                    websocket: false,
                    strip_prefix: false,
                    exact_match: true,
                    health_check_path: None,
                    retry_count: 0,
                    max_inflight: None,
                    upstream_tls: None,
                    timeouts: None,
                    client_max_body_size: None,
                    host_header: None,
                },
            ],
        }],
    }
}

#[test]
fn test_build_router_valid() {
    let conf = mock_config();
    let router = build_router(&conf).expect("Expected valid router");

    // Check wildcard matching for prefix path
    let match1 = router.routes.at("/api/users");
    assert!(match1.is_ok());
    let route1 = match1.unwrap().value;
    assert_eq!(route1.path, "/api");
    assert_eq!(route1.strip_prefix, true);

    // Check exact match only matches precisely
    let match2 = router.routes.at("/exact");
    assert!(match2.is_ok());

    let match3 = router.routes.at("/exact/extra");
    assert!(match3.is_err()); // because exact_match = true
}

#[test]
fn test_build_router_empty_upstreams_skipped() {
    let mut conf = mock_config();
    conf.routes[0].locations[0].upstreams = vec![];

    let router = build_router(&conf).expect("Expected valid router with 1 route");

    // Exact route still exists
    assert!(router.routes.at("/exact").is_ok());
    // API route should be skipped
    assert!(router.routes.at("/api").is_err());
}
