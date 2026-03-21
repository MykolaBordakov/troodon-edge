# Troоdon: Конфігурація та Безпека (L7 / Timeouts)

Цей документ описує всі доступні параметри `config.yaml`: безпека від L7-атак, тайм-аути, TLS, WebSocket, обмеження ресурсів та tracing.

---

## 1. Блок `server` — Глобальна конфігурація

```yaml
server:
  listen_addr: "0.0.0.0"   # IP для прослуховування
  listen_port: 6188         # HTTP порт
  prometheus_port: 9090     # Порт для Prometheus метрик (опціонально)
  log_level: "info"         # Рівень логів: trace | debug | info | warn | error
```

> **Примітка:** `tls_port` переміщено на top-level (поруч з `server:`), а не всередину `server`.

### 1.1 L7 Security (Slowloris, OOM, Concurrency)

Застосовуються глобально до всіх з'єднань, **до маршрутизації**.

```yaml
server:
  # Захист від Slowloris: таймаут на читання заголовків від клієнта.
  client_read_timeout: 10   # секунди

  # Захист від OOM: максимальний сумарний розмір HTTP-заголовків клієнта.
  # Перевищення → HTTP 431.
  max_header_size: 8192     # байти

  # Глобальний ліміт одночасних з'єднань.
  # При перевищенні → HTTP 429. Переконайтеся: OS ulimit -n >= цього значення.
  global_connections: 50000
```

### 1.2 Глобальні тайм-аути (`server.timeouts`)

Fallback-значення для upstream-з'єднань, якщо `Location` не вказав своїх.

```yaml
server:
  timeouts:
    connect: 5    # TCP handshake з бекендом (секунди)
    read: 10      # Перший байт відповіді від бекенду
    write: 10     # Час на запис до бекенду
    idle: 30      # [ВАЖЛИВО] Keep-Alive. Не ставити 0!
```

### 1.3 Глобальна IP Фільтрація (`ip_access_control`)

Працює для всіх запитів до серверу. Перевіряє IP-адресу клієнта ще до початку маршрутизації.

```yaml
server:
  ip_access_control:
    enabled: true
    default_action: "allow" # Що робити, якщо IP немає в списках ("allow" / "deny")
    blacklist:
      - "192.168.1.100"     # Точний IP
      - "10.0.0.0/8"        # Підмережа (CIDR)
    whitelist: []           # Whitelist має вищий пріоритет за Blacklist
```

---

## 2. TLS (HTTPS) — Per-Route, Multi-Domain

HTTPS налаштовується на рівні кожного `route`. Один порт — багато доменів через **SNI**.

```yaml
# Top-level, НЕ всередині server:
tls_port: 6443

routes:
  - host: "api.example.com"
    tls:
      cert: "/etc/ssl/certs/api.crt"
      key:  "/etc/ssl/private/api.key"
    locations: [...]

  - host: "admin.example.com"
    tls:
      cert: "/etc/ssl/certs/admin.crt"
      key:  "/etc/ssl/private/admin.key"
    locations: [...]

  # Без tls: = тільки HTTP
  - host: "internal.svc"
    locations: [...]
```

Детальніше: **[TLS_REFERENCE.md](./TLS_REFERENCE.md)**

---

## 3. Блок `routes` — Маршрутизація

```yaml
routes:
  - host: "api.example.com"   # SNI / Host заголовок для upstream (обов'язково)
    tls:                       # Опціонально — per-route TLS
      cert: "..."
      key:  "..."
      http2: true              # Відкривати ALPN h2 для клієнтів (за замовчуванням false)
      mtls:                    # Опціонально — перевірка клієнтського сертифікату
        enabled: true
        client_ca: "client-ca.crt"
    ip_access_control:         # Опціонально — IP фільтрація саме для цього домену
      enabled: true
      default_action: "deny"   # Блокуємо всіх
      whitelist: ["127.0.0.1"] # Крім цих IP
      blacklist: []
    locations:
      - path: "/api"
        ...
```

### 3.1 Параметри `Location`

| Параметр | Тип | За замовчуванням | Опис |
|---|---|---|---|
| `path` | `string` | `/` | Префіксний або точний шлях |
| `upstreams` | `[string]` | **обов'язковий** | Список `IP:port` або `host:port` бекендів |
| `exact_match` | `bool` | `false` | Тільки точний збіг (без `/*wildcard`) |
| `strip_prefix` | `bool` | `false` | Обрізати `path` з URL перед відправкою upstream |
| `websocket` | `bool` | `false` | Forward `Upgrade`/`Connection` заголовків |
| `health_check_path` | `string?` | `null` | HTTP шлях для Active Health Check |
| `retry_count` | `usize` | `0` | Кількість ретраїв при падінні бекенду |
| `max_inflight` | `isize?` | `null` | Circuit Breaker: max in-flight → 503 |
| `upstream_tls` | `bool?` | `null` | TLS до upstream. `null` = автодетект по порту 443 |
| `upstream_http2`| `bool` | `false` | Увімкнути HTTP/2 з'єднання до бекенду |
| `client_max_body_size` | `usize?` | `null` | Ліміт тіла запиту (байти) → 413 |
| `host_header` | `string?` | `null` | Явний Host заголовок до upstream (якщо відрізняється від SNI) |
| `timeouts` | `Timeouts?` | глобальні | Перекривають глобальні `server.timeouts` |

### 3.2 Route-specific Timeouts (Override)

```yaml
locations:
  - path: "/api"
    upstreams: ["127.0.0.1:8000"]
    timeouts:
      connect: 2
      read: 30
      write: 10
      idle: 60
```

---

## 4. Повний приклад `config.yaml`

```yaml
server:
  listen_addr: "0.0.0.0"
  listen_port: 6188
  prometheus_port: 9090
  log_level: "info"

  client_read_timeout: 10
  max_header_size: 8192
  global_connections: 50000

  timeouts:
    connect: 5
    read: 10
    write: 10
    idle: 30

# HTTPS порт (top-level, не в server:)
tls_port: 6443

routes:
  - host: "api.example.com"
    tls:
      cert: "/etc/ssl/certs/api.crt"
      key:  "/etc/ssl/private/api.key"
      mtls:
        enabled: false
        client_ca: "client-ca.crt"
    ip_access_control:
      enabled: false
      default_action: "allow"
      whitelist: []
      blacklist: []
    locations:

      # REST API
      - path: "/api"
        upstreams: ["127.0.0.1:8000"]
        health_check_path: "/health"
        retry_count: 2
        upstream_http2: true
        client_max_body_size: 1048576  # 1 MB

      # AI endpoint з Circuit Breaker
      - path: "/ai-generate"
        upstreams: ["127.0.0.1:8001"]
        max_inflight: 5
        client_max_body_size: 10485760  # 10 MB
        timeouts:
          connect: 5
          read: 120
          write: 20
          idle: 10

      # WebSocket
      - path: "/chat"
        upstreams: ["127.0.0.1:8002"]
        websocket: true
        timeouts:
          connect: 5
          read: 3600
          write: 10
          idle: 3600

  # HTTP-only маршрут (немає tls:)
  - host: "internal.svc"
    locations:
      - path: "/"
        upstreams: ["10.0.0.1:3000"]
        health_check_path: "/health"
```

---

## 5. Distributed Tracing (X-Request-Id)

Кожен запит автоматично отримує `X-Request-Id` у форматі `{epoch_hex}-{counter_hex}`. Унікальний між рестартами та інстансами (без UUID залежності).

- **Ін'єктується** в upstream запит як `X-Request-Id`
- **Логується** в access log: `ReqID=67c5ee80-000000000000002f`

---

## 6. Prometheus Метрики

| Метрика | Лейбли | Опис |
|---|---|---|
| `troodon_http_requests_total` | `method`, `status`, `host` | Лічильник запитів |
| `troodon_http_request_duration_seconds` | `method`, `status`, `host` | Гістограма латентності |

**`status`** — групується як `2xx`, `3xx`, `4xx`, `5xx` (не окремі коди, щоб уникнути cardinality explosion).

> Детальніше: **[METRICS_REFERENCE.md](./METRICS_REFERENCE.md)**

---

## 7. Graceful Shutdown

При `SIGTERM`: 30s drain → force-close 60s.

Hot-reload routing table (без рестарту): `kill -HUP <PID>`

> ⚠️ **SIGHUP перезавантажує тільки routes.** Зміни `server:` секції (log_level, timeouts, ліміти) потребують повного рестарту.
