# Troоdon: Конфігурація та Безпека (L7 / Timeouts)

Цей документ описує всі доступні параметри `config.yaml`: безпека від L7-атак, тайм-аути, WebSocket, TLS для upstream, обмеження ресурсів та tracing.

---

## 1. Блок `server` — Глобальна конфігурація

```yaml
server:
  listen_addr: "0.0.0.0"   # IP для прослуховування
  listen_port: 6188         # HTTP порт
  tls_port: 6443            # HTTPS порт (опціонально)
  prometheus_port: 9090     # Порт для Prometheus метрик (опціонально)
  log_level: "info"         # Рівень логів: trace | debug | info | warn | error
```

### 1.1 L7 Security (Slowloris, OOM, Concurrency)

Застосовуються глобально до всіх з'єднань, **до маршрутизації**.

```yaml
server:
  # Захист від Slowloris: таймаут на читання заголовків від клієнта.
  # Якщо клієнт не надіслав заголовки за цей час — з'єднання закривається.
  client_read_timeout: 10   # секунди

  # Захист від OOM: максимальний сумарний розмір HTTP-заголовків клієнта.
  # Перевищення → HTTP 431 Request Header Fields Too Large (Early Return).
  max_header_size: 8192     # байти

  # Глобальний ліміт одночасних з'єднань.
  # ⚠️ Тепер це реальний enforcement через Inflight guard:
  # при перевищенні клієнт отримує HTTP 429 Too Many Requests.
  # Переконайтеся, що OS ulimit -n >= цього значення.
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

---

## 2. Блок `routes` — Маршрутизація

```yaml
routes:
  - host: "api.example.com"   # SNI / Host заголовок для upstream
    locations:
      - path: "/api"
        ...
```

### 2.1 Параметри `Location`

| Параметр | Тип | За замовчуванням | Опис |
|---|---|---|---|
| `path` | `string` | `/` | Префіксний або точний шлях |
| `upstreams` | `[string]` | **обов'язковий** | Список `IP:port` або `host:port` бекендів |
| `exact_match` | `bool` | `false` | Якщо `true` — тільки точний збіг (без `/*wildcard`) |
| `strip_prefix` | `bool` | `false` | Обрізати `path` з URL перед відправкою upstream |
| `websocket` | `bool` | `false` | Увімкнути WebSocket (пересилає `Upgrade`/`Connection` заголовки) |
| `health_check_path` | `string?` | `null` | HTTP шлях для Active Health Check. Без нього — відключено |
| `retry_count` | `usize` | `0` | Кількість ретраїв при падінні бекенду (0 = без ретраїв) |
| `max_inflight` | `isize?` | `null` | Circuit Breaker: max in-flight запитів до одного бекенду → 503 |
| `upstream_tls` | `bool?` | `null` | Явний TLS для upstream. Якщо `null` — автодетект по порту 443 |
| `client_max_body_size` | `usize?` | `null` | Максимальний розмір тіла запиту (байти). Перевищення → 413 |
| `timeouts` | `Timeouts?` | глобальні | Перекривають глобальні тайм-аути для цього маршруту |

### 2.2 Route-specific Timeouts (Override)

```yaml
routes:
  - host: "api.example.com"
    locations:
      - path: "/api"
        upstreams: ["127.0.0.1:8000"]
        # Якщо блок відсутній — беруться глобальні server.timeouts
        timeouts:
          connect: 2
          read: 30
          write: 10
          idle: 60
```

---

## 3. Повний приклад `config.yaml`

```yaml
server:
  listen_addr: "0.0.0.0"
  listen_port: 6188
  tls_port: 6443
  prometheus_port: 9090
  log_level: "info"          # Production: "warn"

  # L7 Security
  client_read_timeout: 10
  max_header_size: 8192
  global_connections: 50000  # Enforce через Inflight → 429 при перевищенні

  # Global upstream timeouts (fallback)
  timeouts:
    connect: 5
    read: 10
    write: 10
    idle: 30

tls:
  certificates:
    main_app:
      cert: "/etc/ssl/certs/troodon.crt"
      key: "/etc/ssl/private/troodon.key"

routes:
  - host: "api.example.com"
    locations:

      # Звичайний REST API
      - path: "/api"
        upstreams: ["127.0.0.1:8000"]
        health_check_path: "/health"
        retry_count: 2                 # 2 ретраї при падінні бекенду
        client_max_body_size: 1048576  # 1 MB ліміт на body запиту

      # Важкий AI-генератор з Circuit Breaker
      - path: "/ai-generate"
        upstreams: ["127.0.0.1:8001"]
        max_inflight: 5                # Макс. 5 одночасних запитів → 503
        client_max_body_size: 10485760 # 10 MB
        timeouts:
          connect: 5
          read: 120                    # 2 хвилини на генерацію
          write: 20
          idle: 10

      # WebSocket chat
      - path: "/chat"
        upstreams: ["127.0.0.1:8002"]
        websocket: true                # Forward Upgrade/Connection заголовків
        timeouts:
          connect: 5
          read: 3600                   # 1 година
          write: 10
          idle: 3600

      # External HTTPS upstream (явний TLS, не по порту)
      - path: "/external"
        upstreams: ["api.partner.com:8443"]
        upstream_tls: true             # Явно вмикаємо TLS до upstream
        exact_match: false
```

---

## 4. Distributed Tracing (X-Request-Id)

Кожен запит автоматично отримує унікальний `X-Request-Id` (16-символьний hex, атомарний лічильник). Заголовок:
- **Ін'єктується** в кожен upstream запит як `X-Request-Id`
- **Логується** в кожному рядку access log: `ReqID=000000000000002f`

Бекенд може читати `X-Request-Id` і пробрасувати його далі для end-to-end tracing.

---

## 5. Graceful Shutdown

Troodon налаштований на безпечне завершення при `SIGTERM`:

1. **Зупиняє** прийом нових з'єднань  
2. **Очікує** 30 секунд (`grace_period`) — in-flight запити завершуються
3. **Примусово закриває** через 60 секунд (`graceful_shutdown_timeout`)

Для hot-reload конфіга (тільки routing table) без зупинки: `kill -HUP <PID>`.
