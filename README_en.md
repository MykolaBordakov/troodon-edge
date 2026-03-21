# 🦖 Troodon Edge Proxy

![Status](https://img.shields.io/badge/Status-Active%20Development-success)
![Data Plane](https://img.shields.io/badge/Data%20Plane-Rust%20(Pingora)-orange)
![Control Plane](https://img.shields.io/badge/Control%20Plane-Go-blue)

**Troodon** is a high-performance Edge Proxy built in **Rust** (Pingora), designed for maximum speed, security, and flexibility. Architecture: Rust Data Plane + Go Control Plane + eBPF plugins (roadmap).

## 🌟 Key Features

- **Blazing Fast Data Plane:** Rust + [Pingora](https://github.com/cloudflare/pingora) (Cloudflare). Zero-copy streaming, 100% async.
- **Radix Tree Routing:** O(k) routing via `matchit` — prefix and exact-match routes.
- **Rock-Solid L7 Security:** Slowloris & OOM protection, true global connection limit enforcement (→ HTTP 429).
- **IP Access Control:** Ultra-fast `O(1)` IP filtering (Global and Per-Route) based on an optimized Prefix Trie (`ip_network_table`). Explicit `whitelist` and `blacklist` with customizable `default_action`. Full IPv4 & IPv6 support.
- **Rate Limiting:** Per-IP request limiting (`req_per_sec`) for each route via `pingora-limits`. Protects against DDoS and brute-force (→ HTTP 429).
- **Enforced Security Headers:** Automatic injection of `Strict-Transport-Security`, `X-Content-Type-Options: nosniff`, and `X-Frame-Options: DENY` to all upstream responses.
- **mTLS (Mutual TLS):** Client certificate verification at the domain level (`SslVerifyMode::PEER`). Includes fail-fast boot validation.
- **Cascading Timeouts:** Timeout hierarchy server → location. Individual profiles for WebSocket (hours), REST (seconds), AI endpoints (minutes).
- **Circuit Breaker:** Inflight-limit per backend via `pingora-limits` (→ HTTP 503).
- **Active HTTP Health Checks:** Real HTTP checks with support for custom `Host` headers and status 200 verification. Background service with failure logging.
- **WebSocket Support:** Strict validation — forwards `Upgrade` only if the value is `websocket`. Automatic `Connection: Upgrade` handling.
- **HTTP/2 Support:** Full HTTP/2 capability for clients (downstream) via ALPN and for connecting to backends (`upstream_http2`).
- **Multi-Domain TLS (SNI):** Per-route certificates. One HTTPS port serves N domains. Strict SNI checking: unknown or missing SNI → `TLS ALERT_FATAL`.
- **Upstream TLS:** Explicit `upstream_tls` or port-based auto-detect. Full decoupling of SNI and Host headers via `host_header`.
- **Hot Reload (SIGHUP):** Atomic routing table swapping with zero downtime. Lightweight signal handling via `signal-hook` (independent of main Tokio runtime).
- **Graceful Shutdown (SIGTERM):** 30s drain period, 60s force-close.
- **Distributed Tracing:** Automatic `X-Request-Id` as `{random}-{counter}` — guaranteed unique across restarts and instances.
- **Prometheus Metrics:** `troodon_http_requests_total`, `troodon_http_request_duration_seconds`. Secure binding to `127.0.0.1` by default.
- **Body Size Limiting:** True enforcement via `request_body_filter`, protecting against chunked encoding bypasses (→ HTTP 413).

## 📚 Documentation

- 🇺🇦 [README (Українська)](./README.md) | 🇬🇧 [README (English)](./README_en.md)
- 🛡️ **[Config Reference (English)](./troodon/CONFIG_REFERENCE_en.md)** — all `config.yaml` parameters: IP filtering, L7 security, timeouts, WebSocket.
- 🔒 **[TLS Reference (English)](./troodon/TLS_REFERENCE_en.md)** — downstream/upstream TLS and **mTLS** setup.

## 🚀 Quick Start

```bash
cd troodon
cargo run --release
```

For zero-downtime hot-reload:
```bash
kill -HUP $(pgrep troodon)
```

## 🏗️ Current Status

| Component | Status | Details |
|---|---|---|
| **Data Plane (Rust)** | ✅ Stage 1 Complete | HTTP proxy, Radix routing, L7 security, **IP Filtering (O(1))**, **Rate Limiting**, **Security Headers**, **HTTP/2**, Multi-domain SNI TLS, **mTLS**, WebSocket, Health Checks, Prometheus, Hot Reload, Graceful Shutdown |
| **Control Plane (Go)** | 🔧 In Design | gRPC API for dynamic configuration updates |
| **eBPF Plugins** | 📋 Roadmap | Kernel-level traffic filtering |

---
*Built with ❤️ for ultimate performance.*
