# Troodon: Metrics (Prometheus)

This document describes how metric collection works in the Troodon Edge Proxy, how to enable it, and exactly what we export for monitoring dashboards like Grafana.

---

## 1. How to Enable Metrics

Prometheus metrics are disabled by default. To enable them, you must provide the `prometheus_port` parameter in the global `server` block of the `config.yaml` file:

```yaml
server:
  listen_addr: "0.0.0.0"
  listen_port: 80
  prometheus_port: 9090  # <--- Enables metrics export on port 9090
  prometheus_listen_addr: "127.0.0.1" # Optional: binding IP (default: 127.0.0.1)
  log_level: "info"
```

When the proxy starts with this parameter, the server binds an additional TCP listener exclusively for Prometheus metrics. **Important:** By default, it listens on `127.0.0.1` only to ensure security (metrics isolation). If you need to expose it externally, change `prometheus_listen_addr` to `0.0.0.0`.

In your Prometheus configuration (`scrape_configs`), simply add this port for collection:
```yaml
scrape_configs:
  - job_name: 'troodon_edge'
    static_configs:
      - targets: ['127.0.0.1:9090']
```

---

## 2. What metrics do we collect?

We intercept every connection through our custom proxy framework (built on Pingora) but we intentionally export only **two custom metrics** corresponding to our L7 proxy layer. This guarantees near-zero overhead.

### 2.1 `troodon_http_requests_total`
**Type:** `Counter`
**Description:** The total cumulative number of HTTP requests processed by the proxy since it was started. This is primarily used to calculate overall **RPS (Requests Per Second)**.

### 2.2 `troodon_http_request_duration_seconds`
**Type:** `Histogram`
**Description:** The duration (latency) of HTTP requests from the moment they are initiated to the moment they complete or result in an error. This is crucial for calculating **Latency Percentiles (p50, p90, p99)** of backend response speeds. It provides much more insight than simple averages.

---

## 3. Metric Labels

Both metrics share an identical set of labels, enabling advanced slicing and filtering in Grafana:

| Label | Details / Examples | Description |
|---|---|---|
| `method` | `GET`, `POST`, `OPTIONS` | The HTTP request method. |
| `status` | `1xx`, `2xx`, `3xx`, `4xx`, `5xx`, `unknown` | The HTTP response status code class. |
| `host` | `api.example.com`, `unknown` | The SNI or Host header of the request (`effective_host`). |

> **Why are statuses grouped (e.g. `2xx` instead of `200`)?**
> This is to prevent "cardinality explosion" in the Prometheus TSDB. If an upstream dynamically returns hundreds of different or non-standard status codes, it results in excessive memory load on Prometheus. Instead, we group them into 5 distinct classes. If granular debugging is needed (e.g. distinguishing a 401 from a 404), you should analyze the access logs where the explicit status code is recorded.

---

## 4. Do we export CPU and Memory (RAM) metrics?

**No, the current version of Troodon does not internally export CPU and RAM metrics via the Prometheus port.**

### Why?
Normally, in the Rust ecosystem, the `prometheus` crate includes a `process` feature (a process-collector) which automatically exposes `process_resident_memory_bytes` and `process_cpu_seconds_total`.

However, during development we identified a vulnerability in its downstream dependencies (`protobuf` crate < 3.7.2). To guarantee **uncompromising security** for the Edge Proxy and drop all legacy or vulnerable C++/Rust dependencies, we strictly disabled default features in `Cargo.toml`:
```toml
prometheus = { version = "0.13", default-features = false }
```
This disabled the automatic collection of standard process metrics from within the application itself.

### How to monitor CPU / RAM for Troodon?
An Edge Proxy is typically deployed in containers (Docker / Kubernetes) or as a Linux Systemd service. In Production environments, monitoring OS-level resources is an infrastructural responsibility:

1. **Kubernetes:** Utilize standard `cAdvisor` metrics (e.g. `container_memory_working_set_bytes` or `container_cpu_usage_seconds_total`), filtering by pod or container name.
2. **Docker:** Utilize `cAdvisor` or `docker stats`.
3. **Linux / Systemd:** Utilize the Prometheus `node_exporter`, which collects detailed OS process information without baking that logic into the core load balancer binary (similar to how Nginx or Envoy operate).

This design aligns with best practices around Separation of Concerns (SoC). The proxy exclusively exports business and L7 technical metrics (RPS, Latency), while the infrastructure handles CPU/RAM profiling.
