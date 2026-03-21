# Troodon: TLS (HTTPS) Configuration

## 1. Architecture

Troodon uses **OpenSSL** (via Pingora) for TLS termination. It supports **SNI-based multi-cert** natively — each domain receives its own distinct certificate on a single shared port.

**How it works:**
1. At startup, all configured certificates are loaded into memory via `SslContextBuilder` (zero I/O on the hot path)
2. The client connects and sends the SNI (Extension in the TLS ClientHello)
3. Troodon matches the corresponding `SslContext` and returns the correct certificate
4. **Missing SNI or unknown domain → `TLS ALERT_FATAL`** — there are no fallback or default certificates

---

## 2. Downstream TLS (Client → Troodon)

TLS is configured **per-route**, not globally. Each domain uses its own certificate.

```yaml
# Global HTTPS port (one port for all domains)
tls_port: 6443

routes:
  # Domain 1 — its own certificate
  - host: "api.example.com"
    tls:
      cert: "/etc/ssl/certs/api.crt"    # PEM Full Chain
      key:  "/etc/ssl/private/api.key"  # PEM Private Key (unencrypted)
      http2: true                       # Enable HTTP/2 via ALPN
    locations:
      - path: "/api"
        upstreams: ["backend:8080"]

  # Domain 2 — different certificate, same port
  - host: "admin.example.com"
    tls:
      cert: "/etc/ssl/certs/admin.crt"
      key:  "/etc/ssl/private/admin.key"
    locations:
      - path: "/"
        upstreams: ["admin-backend:9000"]

  # Without TLS — HTTP only (via port 6188)
  - host: "internal.svc"
    # missing tls: block = HTTP only
    locations:
      - path: "/"
        upstreams: ["internal:3000"]
```

### File Formats

- **`cert`**: `.crt`/`.pem` containing `-----BEGIN CERTIFICATE-----`. Full Chain (domain + intermediate CAs) is strongly recommended.
- **`key`**: `.key`/`.pem` containing `-----BEGIN PRIVATE KEY-----`. Passphrase-protected keys are **not supported**.
- **`client_ca`** (only with `mtls.enabled: true`): The CA file used to sign and verify client certificates.

### mTLS (Mutual TLS: Client Verification)

If you need to cryptographically authenticate clients, you can specify the `mtls` block inside the `tls` config.

```yaml
    tls:
      cert: "/etc/ssl/certs/admin.crt"
      key:  "/etc/ssl/private/admin.key"
      mtls:
        enabled: true
        client_ca: "/etc/ssl/certs/my_company_client_ca.crt"
```

When `mtls.enabled: true`:
- The server will request and verify a client certificate during the TLS handshake.
- The client connection will be dropped (`TLS ALERT_FATAL`) if the certificate is missing or if it has not been signed by the exact `client_ca` configured.
- **Fail Fast:** If you enable mTLS but the `client_ca` file is missing on disk, Troodon will **refuse to start** to prevent accidental downgrades in security.

### SNI — Strict Validation

| Scenario | Result|
|---|---|
| SNI exists, domain is configured | ✅ Serves the correct certificate |
| SNI exists, domain is **not** configured | ❌ `TLS ALERT_FATAL` |
| SNI **missing** (legacy clients) | ❌ `TLS ALERT_FATAL` |

> **Note:** All modern browsers and HTTP clients send SNI natively. The absence of an SNI typically indicates an obsolete or misconfigured client.

### Testing Validation

```bash
# Testing with an explicit SNI
curl -v -k --resolve api.example.com:6443:127.0.0.1 https://api.example.com:6443/api

# Testing without SNI (should fail)
curl -v -k https://127.0.0.1:6443/api
```

Expected Startup Logs:
```
🔒 TLS cert loaded for domain: api.example.com
🔒 TLS cert loaded for domain: admin.example.com
🔒 Troodon is binding to TLS (HTTPS): 0.0.0.0:6443 (2 domain(s))
```

---

## 3. Upstream TLS (Troodon → Backend)

Connecting to backends securely via HTTPS is configured using the `upstream_tls` parameter in the `locations` block:

```yaml
routes:
  - host: "api.example.com"
    locations:
      # Explicit TLS on any port
      - path: "/secure"
        upstreams: ["internal-api:8443"]
        upstream_tls: true          # Explicitly enable TLS

      # Auto-detect: if port is 443 and upstream_tls is not specified
      - path: "/external"
        upstreams: ["api.partner.com:443"]
        # upstream_tls is omitted → automatically true (because port 443)

      # Explicit HTTP (to prevent ambiguity with the port)
      - path: "/internal"
        upstreams: ["10.0.0.1:443"]
        upstream_tls: false         # Explicitly disable TLS
```

> **Note:** `upstream_tls` takes precedence over auto-detection. The SNI header passed to the upstream is taken from the route's `host` field (or the overridden `host_header`).

---

## 4. Test Certificate Generation

```bash
openssl req -x509 -newkey rsa:4096 -keyout test.key -out test.crt \
  -days 365 -nodes -subj "/CN=localhost"
```
