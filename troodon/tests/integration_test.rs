use reqwest::Client;
use std::fs::File;
use std::io::Write;
use std::process::{Child, Command};
use std::time::Duration;
use warp::Filter;

// Helper to spawn a mock backend on a random port
async fn spawn_mock_backend() -> (u16, tokio::task::JoinHandle<()>) {
    let route = warp::any()
        .map(|| warp::reply::with_status("Mock Backend Response", warp::http::StatusCode::OK));
    let (addr, server) = warp::serve(route).bind_ephemeral(([127, 0, 0, 1], 0));
    let handle = tokio::spawn(server);
    (addr.port(), handle)
}

struct ProxyProcess {
    child: Child,
    config_path: String,
}

impl Drop for ProxyProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.config_path);
    }
}

async fn start_proxy(backend_port: u16, config_idx: usize) -> (ProxyProcess, u16) {
    let proxy_port = 18000 + config_idx as u16;
    let config_path = format!("target/test_config_{}.yaml", config_idx);

    let config_content = format!(
        r#"
server:
  listen_addr: "127.0.0.1"
  listen_port: {}
  log_level: "debug"
routes:
  - host: "localhost"
    locations:
      - path: "/"
        upstreams: ["127.0.0.1:{}"]
        exact_match: false
"#,
        proxy_port, backend_port
    );

    let mut file = File::create(&config_path).unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let exe = env!("CARGO_BIN_EXE_troodon");

    // Spawn proxy
    let child = Command::new(exe)
        .env("TROODON_CONFIG", &config_path)
        .env("RUST_LOG", "debug")
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("Failed to spawn proxy");

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(1500)).await;

    (ProxyProcess { child, config_path }, proxy_port)
}

#[tokio::test]
async fn test_basic_routing() {
    let (backend_port, _backend_handle) = spawn_mock_backend().await;
    let (_proxy, proxy_port) = start_proxy(backend_port, 1).await;

    let client = Client::new();
    let res = client
        .get(format!("http://127.0.0.1:{}", proxy_port))
        .header("Host", "localhost")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(res.status().as_u16(), 200);
    assert!(res.headers().contains_key("X-Proxy-By"));

    let text = res.text().await.unwrap();
    assert_eq!(text, "Mock Backend Response");
}
