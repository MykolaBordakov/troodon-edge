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
- **Multi-Domain TLS (SNI):** Per-route сертифікати — кожен домен має свій cert/key. Один HTTPS порт обслуговує N доменів. Строга SNI перевірка: невідомий домен або відсутній SNI → `TLS ALERT_FATAL`.
- **Upstream TLS:** Явний `upstream_tls` або автодетект по порту 443. Підтримка `host_header` для розділення SNI і Host заголовку.
- **Hot Reload (SIGHUP):** Атомарна заміна routing table без даунтайму через `ArcSwap`.
- **Graceful Shutdown (SIGTERM):** Drain period 30s, force-close 60s.
- **Distributed Tracing:** Автоматичний `X-Request-Id` у форматі `{epoch}-{counter}` — унікальний між рестартами та інстансами.
- **Prometheus Metrics:** `troodon_http_requests_total`, `troodon_http_request_duration_seconds`. Статус групується як `2xx/3xx/4xx/5xx`.
- **Body Size Limiting:** Real enforcement через `request_body_filter` — захищає від chunked encoding bypass (→ HTTP 413).

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
| **Data Plane (Rust)** | ✅ Stage 1 Complete | HTTP proxy, Radix routing, L7 security, Multi-domain SNI TLS, WebSocket, Health Checks, Prometheus (2xx/3xx/4xx/5xx grouping), Hot Reload (routes-only), Graceful Shutdown, X-Request-Id (epoch+counter), Body Limit (chunked-safe), Circuit Breaker, host_header decoupling |
| **Control Plane (Go)** | 🔧 In Design | gRPC API для динамічного оновлення конфігурації |
| **eBPF Plugins** | 📋 Roadmap | Kernel-level traffic filtering |

---
*Built with ❤️ for ultimate performance.*
