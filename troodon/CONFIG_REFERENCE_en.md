# Troodon: Configuration and Security (L7 / Timeouts)

This document describes all available `config.yaml` parameters: L7 security, timeouts, TLS, WebSocket, resource limits, and tracing.

---

## 1. `server` block — Global Configuration

```yaml
server:
  listen_addr: "0.0.0.0"   # IP to listen on
  listen_port: 6188         # HTTP port
  prometheus_port: 9090     # Prometheus metrics port (optional)
  log_level: "info"         # Log level: trace | debug | info | warn | error
```

> **Note:** `tls_port` is moved to the top-level (next to `server:`), not inside `server:`.

### 1.1 L7 Security (Slowloris, OOM, Concurrency)

Applies globally to all connections, **before routing**.

```yaml
server:
  # Slowloris protection: timeout for reading client headers.
  client_read_timeout: 10   # seconds

  # OOM protection: maximum total size of client HTTP headers.
  # Exceeding this returns HTTP 431.
  max_header_size: 8192     # bytes

  # Global concurrent connection limit.
  # Exceeding this returns HTTP 429. Ensure: OS ulimit -n >= this value.
  global_connections: 50000
```

### 1.2 Global Timeouts (`server.timeouts`)

Fallback values for upstream connections, if a `Location` does not specify its own.

```yaml
server:
  timeouts:
    connect: 5    # TCP handshake with the backend (seconds)
    read: 10      # Time to first byte of the response
    write: 10     # Time to write request to the backend
    idle: 30      # [IMPORTANT] Keep-Alive idle timeout. Do not set to 0!
```

### 1.3 Global IP Filtering (`ip_access_control`)

Applies to all requests to the server. Checks the client's IP address even before routing starts.

```yaml
server:
  ip_access_control:
    enabled: true
    default_action: "allow" # What to do if IP is not in the lists ("allow" or "deny")
    blacklist:
      - "192.168.1.100"     # Exact IP
      - "10.0.0.0/8"        # Subnet (CIDR)
    whitelist: []           # Whitelist has higher priority than Blacklist
```

---

## 2. TLS (HTTPS) — Per-Route, Multi-Domain

HTTPS is configured at the level of each `route`. One port — multiple domains via **SNI**.

```yaml
# Top-level, NOT inside server:
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

  # Missing tls: block = HTTP only
  - host: "internal.svc"
    locations: [...]
```

For more details: **[TLS_REFERENCE_en.md](./TLS_REFERENCE_en.md)**

---

## 3. `routes` block — Routing

```yaml
routes:
  - host: "api.example.com"   # SNI / Host header for upstream (required)
    tls:                       # Optional — per-route TLS
      cert: "..."
      key:  "..."
      http2: true              # Allow ALPN h2 for clients (default: false)
      mtls:                    # Optional — Mutual TLS (Client certificate verification)
        enabled: true
        client_ca: "client-ca.crt"
    ip_access_control:         # Optional — IP filtering specifically for this domain
      enabled: true
      default_action: "deny"   # Block everyone by default
      whitelist: ["127.0.0.1"] # Except these IPs
      blacklist: []
    locations:
      - path: "/api"
        ...
```

### 3.1 `Location` Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `path` | `string` | `/` | Prefix or exact path |
| `upstreams` | `[string]` | **required** | List of `IP:port` or `host:port` backends |
| `exact_match` | `bool` | `false` | Exact match only (disables `/*wildcard`) |
| `strip_prefix` | `bool` | `false` | Strip `path` from the URL before sending upstream |
| `websocket` | `bool` | `false` | Forward `Upgrade`/`Connection` headers |
| `health_check_path` | `string?` | `null` | HTTP path for Active Health Check |
| `retry_count` | `usize` | `0` | Number of retries when backend falls |
| `max_inflight` | `isize?` | `null` | Circuit Breaker: max in-flight requests → 503 |
| `upstream_tls` | `bool?` | `null` | TLS connection to upstream. `null` = auto-detect via port 443 |
| `upstream_http2`| `bool` | `false` | Enable HTTP/2 connections to the backend |
| `client_max_body_size` | `usize?` | `null` | Max request body limit (bytes) → 413 |
| `host_header` | `string?` | `null` | Explicit Host header to upstream (if different from SNI) |
| `timeouts` | `Timeouts?` | global | Overrides the global `server.timeouts` |

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

## 4. Full `config.yaml` Example

```yaml
server:
  listen_addr: "0.0.0.0"
  listen_port: 6188
  prometheus_port: 9090
  log_level: "info"

  client_read_timeout: 10
  max_header_size: 8192
  global_connections: 50000

  ip_access_control:
    enabled: true
    default_action: "allow"
    blacklist: ["10.0.0.0/8"]

  timeouts:
    connect: 5
    read: 10
    write: 10
    idle: 30

# HTTPS port (top-level, not in server:)
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

      # AI endpoint with Circuit Breaker
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

  # HTTP-only route (no tls: block)
  - host: "internal.svc"
    locations:
      - path: "/"
        upstreams: ["10.0.0.1:3000"]
        health_check_path: "/health"
```

---

## 5. Distributed Tracing (X-Request-Id)

Every request is automatically assigned an `X-Request-Id` in the format `{epoch_hex}-{counter_hex}`. This is uniquely guaranteed between restarts and instances (without the heavy UUID dependencies).

- **Injected** into upstream requests as the `X-Request-Id` header.
- **Logged** in the access logs: `ReqID=67c5ee80-000000000000002f`

---

## 6. Prometheus Metrics

| Metric | Labels | Description |
|---|---|---|
| `troodon_http_requests_total` | `method`, `status`, `host` | Requests counter |
| `troodon_http_request_duration_seconds` | `method`, `status`, `host` | Latency histogram |

**`status`** is grouped logically as `2xx`, `3xx`, `4xx`, `5xx` (individual codes are bypassed to prevent cardinality explosion).

> For more details: **[METRICS_REFERENCE_en.md](./METRICS_REFERENCE_en.md)**

---

## 7. Graceful Shutdown

On `SIGTERM`: 30s connection draining → force-close at 60s.

Hot-reload routing table (without server restart): `kill -HUP <PID>`

> ⚠️ **SIGHUP only reloads the `routes` section.** Changes to the `server:` block (log levels, timeouts, security limits) strictly require a full process restart.
