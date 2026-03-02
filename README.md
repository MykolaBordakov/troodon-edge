# 🦖 Troodon Edge Proxy

![Status](https://img.shields.io/badge/Status-Active%20Development-success)
![Data Plane](https://img.shields.io/badge/Data%20Plane-Rust%20(Pingora)-orange)
![Control Plane](https://img.shields.io/badge/Control%20Plane-Go-blue)

**Troodon** — високопродуктивний Edge Proxy на **Rust** (Pingora), розроблений для максимальної швидкості, безпеки та гнучкості. Архітектура: Rust Data Plane + Go Control Plane + eBPF-плагіни (roadmap).

## 🌟 Ключові можливості

- **Blazing Fast Data Plane:** Rust + [Pingora](https://github.com/cloudflare/pingora) (Cloudflare). Zero-copy streaming, 100% async.
- **Radix Tree Routing:** O(k) маршрутизація через `matchit` — prefix та exact-match маршрути.
- **Rock-Solid L7 Security:** Slowloris protection (client read timeout), OOM protection (max header size), реальний enforcement global connection limit (→ HTTP 429).
- **Cascading Timeouts:** Ієрархія тайм-аутів server → location. Індивідуальні профілі для WebSocket (hours), REST (seconds), AI endpoints (minutes).
- **Circuit Breaker:** Inflight-ліміт на бекенд через `pingora-limits` (→ HTTP 503).
- **Active HTTP Health Checks:** Реальні HTTP-перевірки (не TCP-only) з налаштованим `health_check_path`. Фонова служба кожні 5 секунд.
- **WebSocket Support:** Автоматичний forward `Upgrade`/`Connection` заголовків при `websocket: true`.
- **TLS Termination:** OpenSSL через Pingora. Upstream TLS через явний `upstream_tls` або автодетект по порту 443.
- **Hot Reload (SIGHUP):** Атомарна заміна routing table без даунтайму через `ArcSwap`.
- **Graceful Shutdown (SIGTERM):** Drain period 30s, force-close 60s.
- **Distributed Tracing:** Автоматичний `X-Request-Id` (атомарний hex лічильник) у кожному запиті та access log.
- **Prometheus Metrics:** `troodon_http_requests_total`, `troodon_http_request_duration_seconds` з лейблами method/status/host.
- **Body Size Limiting:** `client_max_body_size` per-location (→ HTTP 413).

## 📚 Документація

- 🛡️ **[Конфігурація (Config Reference)](./troodon/CONFIG_REFERENCE.md)** — всі параметри config.yaml: L7 security, timeouts, WebSocket, circuit breakers, tracing.
- 🔒 **[TLS (HTTPS Termination)](./troodon/TLS_REFERENCE.md)** — downstream і upstream TLS, `upstream_tls` прапорець.

## 🚀 Швидкий старт

```bash
cd troodon
cargo run --release
```

Для hot-reload без рестарту:
```bash
kill -HUP $(pgrep troodon)
```

## 🏗️ Поточний статус

| Компонент | Статус | Деталі |
|---|---|---|
| **Data Plane (Rust)** | ✅ Stage 1 Complete | HTTP proxy, Radix routing, L7 security, TLS, WebSocket, Health Checks, Prometheus, Hot Reload, Graceful Shutdown, X-Request-Id, Body Limit, Circuit Breaker |
| **Control Plane (Go)** | 🔧 In Design | gRPC API для динамічного оновлення конфігурації |
| **eBPF Plugins** | 📋 Roadmap | Kernel-level traffic filtering |

---
*Built with ❤️ for ultimate performance.*
