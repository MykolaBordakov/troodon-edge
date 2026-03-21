# 🦖 Troodon Edge Proxy

![Status](https://img.shields.io/badge/Status-Active%20Development-success)
![Data Plane](https://img.shields.io/badge/Data%20Plane-Rust%20(Pingora)-orange)
![Control Plane](https://img.shields.io/badge/Control%20Plane-Go-blue)

**Troodon** — високопродуктивний Edge Proxy на **Rust** (Pingora), розроблений для максимальної швидкості, безпеки та гнучкості. Архітектура: Rust Data Plane + Go Control Plane + eBPF-плагіни (roadmap).

## 🌟 Ключові можливості

- **Blazing Fast Data Plane:** Rust + [Pingora](https://github.com/cloudflare/pingora) (Cloudflare). Zero-copy streaming, 100% async.
- **Radix Tree Routing:** O(k) маршрутизація через `matchit` — prefix та exact-match маршрути.
- **Rock-Solid L7 Security:** Slowloris protection, OOM protection, та глобальний enforcement connection limit (→ HTTP 429). 
- **IP Access Control:** Надшвидка `O(1)/O(log n)` дворівнева IP-фільтрація (Global та Per-Route) на базі Radix-дерева (`ipnet`). Окремі `whitelist` і `blacklist` з явною політикою `default_action`.
- **mTLS (Mutual TLS):** Підтримка перевірки клієнтських сертифікатів на рівні домену (`SslVerifyMode::PEER`). Fail-fast валідація при старті.
- **Cascading Timeouts:** Ієрархія тайм-аутів server → location. Індивідуальні профілі для WebSocket (hours), REST (seconds), AI endpoints (minutes).
- **Circuit Breaker:** Inflight-ліміт на бекенд через `pingora-limits` (→ HTTP 503).
- **Active HTTP Health Checks:** Реальні HTTP-перевірки (не TCP-only) з налаштованим `health_check_path`. Фонова служба кожні 5 секунд.
- **WebSocket Support:** Автоматичний forward `Upgrade`/`Connection` заголовків при `websocket: true`.
- **HTTP/2 Support:** Підтримка HTTP/2 для клієнтів (downstream) через ALPN та підтримка HTTP/2 для бекендів (`upstream_http2`).
- **Multi-Domain TLS (SNI):** Per-route сертифікати — кожен домен має свій cert/key. Один HTTPS порт обслуговує N доменів. Строга SNI перевірка: невідомий домен або відсутній SNI → `TLS ALERT_FATAL`.
- **Upstream TLS:** Явний `upstream_tls` або автодетект по порту 443. Підтримка `host_header` для розділення SNI і Host заголовку.
- **Hot Reload (SIGHUP):** Атомарна заміна routing table без даунтайму через `ArcSwap`.
- **Graceful Shutdown (SIGTERM):** Drain period 30s, force-close 60s.
- **Distributed Tracing:** Автоматичний `X-Request-Id` у форматі `{epoch}-{counter}` — унікальний між рестартами та інстансами.
- **Prometheus Metrics:** `troodon_http_requests_total`, `troodon_http_request_duration_seconds`. Статус групується як `2xx/3xx/4xx/5xx`.
- **Body Size Limiting:** Real enforcement через `request_body_filter` — захищає від chunked encoding bypass (→ HTTP 413).

## 📚 Документація / Documentation

- 🇺🇦 [README (Українська)](./README.md) | 🇬🇧 [README (English)](./README_en.md)
- 🛡️ **[Конфігурація (Config Reference)](./troodon/CONFIG_REFERENCE.md)** — параметри config.yaml: L7 security, IP filtering, timeouts, WebSocket.
- 🔒 **[TLS (HTTPS Termination)](./troodon/TLS_REFERENCE.md)** — downstream/upstream TLS та **mTLS**.
- 📊 **[Метрики (Metrics Reference)](./troodon/METRICS_REFERENCE.md)** — які L7 метрики віддаються Prometheus (RPS, Latency, Status).

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
| **Data Plane (Rust)** | ✅ Stage 1 Complete | HTTP proxy, Radix routing, L7 security, **IP Filtering**, **HTTP/2**, Multi-domain SNI TLS, **mTLS**, WebSocket, Health Checks, Prometheus, Hot Reload, Graceful Shutdown, X-Request-Id |
| **Control Plane (Go)** | 🔧 In Design | gRPC API для динамічного оновлення конфігурації |
| **eBPF Plugins** | 📋 Roadmap | Kernel-level traffic filtering |

---
*Built with ❤️ for ultimate performance.*
