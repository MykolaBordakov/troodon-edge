use arc_swap::ArcSwap;
use async_trait::async_trait;
use matchit::Router;
use pingora::lb::LoadBalancer;
use pingora::lb::selection::RoundRobin;
use pingora::prelude::*;
use pingora::upstreams::peer::Peer;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

use prometheus::{
    HistogramVec, IntCounterVec, Opts, register_histogram_vec, register_int_counter_vec,
};

static REQ_COUNTER: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        Opts::new(
            "troodon_http_requests_total",
            "Total number of HTTP requests"
        ),
        &["method", "status", "host"]
    )
    .expect("Failed to create metric REQ_COUNTER")
});

static REQ_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        "troodon_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "status", "host"]
    )
    .expect("Failed to create metric REQ_DURATION")
});

use crate::config::ServerConfig;

// Лічильник для генерації унікальних Request ID (без зовнішніх залежностей)
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// Час старту процесу у секундах від Unix Epoch — префікс для Request ID.
// Гарантує унікальність між рестартами/інстансами (без UUID залежності).
static PROCESS_START_SECS: LazyLock<u64> = LazyLock::new(|| {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
});

// === СТРУКТУРИ ===

// 1. Опис маршруту (те, що ми підготували в main.rs)
pub struct ProxyRoute {
    pub path: String,
    pub lb: Arc<LoadBalancer<RoundRobin>>,
    pub sni: String,
    // Явний Host заголовок для upstream. Якщо None — використовується sni.
    pub host_header: Option<String>,
    pub strip_prefix: bool,
    pub max_inflight: Option<isize>,
    pub timeouts: crate::config::Timeouts,
    pub retry_count: usize,
    pub upstream_tls: Option<bool>,
    pub websocket: bool,
    pub client_max_body_size: Option<usize>,
}

// 2. Контекст запиту (наш "кошик" для передачі даних між етапами)
pub struct ProxyContext {
    pub sni: String,
    // Значення, яке реально пишеться у заголовок Host upstream запиту
    pub effective_host: String,
    pub strip_prefix: bool,
    pub path_prefix: String,
    pub inflight_guard: Option<pingora_limits::inflight::Guard>,
    pub start_time: Instant,
    pub retries_left: usize,
    // Унікальний ID запиту для distributed tracing: <epoch_secs>-<counter>
    pub request_id: String,
    // Guard глобального ліміту конекцій
    pub global_guard: Option<pingora_limits::inflight::Guard>,
    // Прапорець WebSocket для спеціальної обробки заголовків
    pub websocket: bool,
    // Лічильник реального розміру тіла запиту (байти, для chunked encoding)
    pub body_bytes_received: usize,
    // Максимально допустимий розмір тіла (None = без обмеження)
    pub max_body_size: Option<usize>,
}

// 3. Роутер, який містить Radix-дерево
pub struct ProxyRouter {
    pub routes: Router<Arc<ProxyRoute>>,
    pub health_checks: Vec<(String, Arc<LoadBalancer<RoundRobin>>)>,
}

// 4. Головна структура балансувальника
pub struct LB {
    pub router: Arc<ArcSwap<ProxyRouter>>,
    pub config: Arc<ServerConfig>,
    pub inflight: Arc<pingora_limits::inflight::Inflight>,
}

#[async_trait]
impl ProxyHttp for LB {
    // === ВАЖЛИВО: Визначаємо наш тип контексту ===
    type CTX = ProxyContext;

    // Ініціалізуємо контекст порожнім
    fn new_ctx(&self) -> Self::CTX {
        ProxyContext {
            sni: String::new(),
            effective_host: String::new(),
            strip_prefix: false,
            path_prefix: String::new(),
            inflight_guard: None,
            start_time: Instant::now(),
            retries_left: 0,
            request_id: String::new(),
            global_guard: None,
            websocket: false,
            body_bytes_received: 0,
            max_body_size: None,
        }
    }

    // 0. ФІЛЬТР ЗАПИТУ (Рання валідація L7 Security)
    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        // Генеруємо унікальний Request ID для кожного запиту.
        // Формат: <process_start_epoch_hex>-<counter_hex>.
        // process_start_epoch гарантує унікальність між рестартами та інстансами.
        let req_num = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        ctx.request_id = format!("{:08x}-{:016x}", *PROCESS_START_SECS, req_num);

        // #10: Enforce глобальний ліміт конекцій
        if let Some(max_conn) = self.config.global_connections {
            let (guard, current) = self.inflight.incr("::global".to_string(), 1);
            if current > max_conn as isize {
                drop(guard); // ҉о не зберігаємо guard, лічильник відразу decr
                warn!(
                    "🛑 Global connection limit exceeded ({}/{} active). Returning 429. ReqID={}",
                    current, max_conn, ctx.request_id
                );
                let _ = session.respond_error(429).await;
                return Ok(true);
            }
            ctx.global_guard = Some(guard); // Живе до кінця запиту, авто-decr через Drop
        }

        // 1. Захист від Slowloris: Встановлюємо тайм-аут на читання заголовків (Client Read Timeout)
        if let Some(timeout) = self.config.client_read_timeout {
            session.set_read_timeout(Some(std::time::Duration::from_secs(timeout)));
            debug!("Set client read timeout to {}s", timeout);
        }

        // 2. Захист від OOM: Перевірка максимального розміру заголовків
        if let Some(max_size) = self.config.max_header_size {
            let mut current_size = 0;
            current_size += session.req_header().method.as_str().len();
            current_size += session
                .req_header()
                .uri
                .path_and_query()
                .map_or(0, |pq| pq.as_str().len());

            for (k, v) in session.req_header().headers.iter() {
                current_size += k.as_str().len() + v.len() + 2; // + ': '
            }

            if current_size > max_size {
                warn!(
                    "🛑 Rejecting request: Headers too large ({} bytes > {} bytes max) ReqID={}",
                    current_size, max_size, ctx.request_id
                );
                let _ = session.respond_error(431).await;
                return Ok(true);
            }
        }

        Ok(false)
    }

    // 1. ВИБІР БЕКЕНДУ
    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX, // Отримуємо доступ до контексту
    ) -> Result<Box<HttpPeer>> {
        // Зберігаємо час початку запиту в context (використовуємо pingora context cache замість кастомних полів, або додамо поле в ProxyContext)
        // Для простоти, додаватимемо поле `start_time` у ProxyContext.

        let path = session.req_header().uri.path();

        // Атомарно читаємо поточний роутер
        let router_guard = self.router.load();

        debug!("🔍 Routing path: '{}'", path);

        // Шукаємо маршрут через Radix-дерево (O(k), де k - довжина шляху)
        let route = match router_guard.routes.at(path) {
            Ok(found) => {
                debug!("✅ Matchit found match for path: '{}'", path);
                found.value
            }
            Err(e) => {
                error!("❌ Matchit rejected path '{}' with error: {:?}", path, e); // NEW DEBUG LOG
                warn!("No route found for path: {}", path);
                // Повертаємо 404, якщо маршрут не знайдено
                return Err(pingora::Error::new(pingora::ErrorType::HTTPStatus(404)));
            }
        };

        debug!("Path '{}' matched route '{}'", path, route.path);

        // === МАГІЯ ТУТ ===
        // Зберігаємо знайдений SNI в контекст, щоб використати пізніше
        ctx.sni = route.sni.clone();
        // Визначаємо ефективний Host заголовок: явний host_header або SNI
        ctx.effective_host = route
            .host_header
            .clone()
            .unwrap_or_else(|| route.sni.clone());
        ctx.strip_prefix = route.strip_prefix; // Чи різати?
        ctx.path_prefix = route.path.clone(); // Що різати?

        let upstream = match route.lb.select(b"", 256) {
            Some(u) => u,
            None => {
                error!(
                    "❌ No healthy backends available for route '{}'. Returning 502.",
                    route.path
                );
                return Err(pingora::Error::new(pingora::ErrorType::HTTPStatus(502)));
            }
        };
        debug!(
            "Load Balancer selected upstream: {:?}",
            upstream.addr.as_inet()
        );

        // Зберігаємо WebSocket прапорець в контекст
        ctx.websocket = route.websocket;

        // Зберігаємо лічильник ретраїв з конфігу маршруту
        ctx.retries_left = route.retry_count;

        // Зберігаємо ліміт тіла в контекст для обробки в request_body_filter().
        // Швидка перевірка Content-Length (soft check): легко обійти через chunked,
        // але відсікає "чесних" клієнтів ще до з'єднання з upstream.
        ctx.max_body_size = route.client_max_body_size;
        if let Some(max_body) = route.client_max_body_size {
            let content_length = session
                .req_header()
                .headers
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);

            if content_length > max_body {
                warn!(
                    "🛑 Request body too large via Content-Length ({} > {} bytes). Returning 413. ReqID={}",
                    content_length, max_body, ctx.request_id
                );
                return Err(pingora::Error::new(pingora::ErrorType::HTTPStatus(413)));
            }
        }

        // Визначаємо, чи потрібен TLS: експліцитний upstream_tls або fallback на порт 443
        let use_tls = match route.upstream_tls {
            Some(explicit) => explicit,
            None => match upstream.addr.as_inet() {
                Some(inet) => inet.port() == 443,
                None => false,
            },
        };

        // --- CIRCUIT BREAKER (pingora-limits) ---
        // Відстеження Inflight запитів для запобігання перевантаження "завислого" бекенду.
        if let Some(max_conn) = route.max_inflight {
            // Отримуємо унікальний ключ для бекенду (наприклад, його IP:Port строку)
            let backend_key = upstream.addr.to_string();
            // Збільшуємо лічильник для цього бекенду
            let (guard, current_inflight) = self.inflight.incr(backend_key.clone(), 1);

            if current_inflight > max_conn {
                warn!(
                    "🛑 Backend {} overloaded. Current inflight ({} > {}). Rejecting request with 503.",
                    backend_key, current_inflight, max_conn
                );
                return Err(pingora::Error::new(pingora::ErrorType::HTTPStatus(503)));
            }

            // Якщо все добре, зберігаємо guard в контекст.
            // Щойно клієнт відключиться або запит завершиться, `ctx` знищиться і guard викличе decr() автоматично.
            ctx.inflight_guard = Some(guard);
        }
        // ----------------------------------------

        // SNI передаємо тільки якщо використовується TLS
        let peer_sni = if use_tls {
            route.sni.clone()
        } else {
            String::new()
        };

        let mut peer = Box::new(HttpPeer::new(upstream.addr, use_tls, peer_sni.clone()));
        peer.sni = peer_sni; // Дублюємо SNI для сумісності з Pingora internal

        // Тайм-аути із маршруту (Route Specific)
        let timeouts = &route.timeouts;
        peer.options.connection_timeout = Some(std::time::Duration::from_secs(timeouts.connect));
        peer.options.read_timeout = Some(std::time::Duration::from_secs(timeouts.read));
        peer.options.write_timeout = Some(std::time::Duration::from_secs(timeouts.write));
        peer.options.idle_timeout = Some(std::time::Duration::from_secs(timeouts.idle));

        Ok(peer)
    }

    // 2. МОДИФІКАЦІЯ ЗАГОЛОВКІВ (Header Injection)
    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX, // Читаємо контекст
    ) -> Result<()> {
        // --- PRODUCTION HEADERS ---
        // Використовуємо effective_host (може відрізнятись від SNI через host_header в конфігу)
        if !ctx.effective_host.is_empty() {
            upstream_request.insert_header("Host", &ctx.effective_host)?;
        } else {
            // Якщо обидва порожні — щось пішло не так. Логуємо і не надсилаємо Host,
            // що призведе до 400 на upstream. Це краще ніж тихо надсилати неправильний Host.
            error!(
                "effective_host is empty for request ReqID={}. upstream may return 400.",
                ctx.request_id
            );
        }

        // X-Real-IP / X-Forwarded-For
        if let Some(client_ip) = session.client_addr()
            && let Some(ip) = client_ip.as_inet()
        {
            let ip_str = ip.ip().to_string();
            upstream_request.insert_header("X-Real-IP", &ip_str)?;
            upstream_request.insert_header("X-Forwarded-For", &ip_str)?;
        }

        // #15: X-Request-Id для distributed tracing
        upstream_request.insert_header("X-Request-Id", &ctx.request_id)?;
        upstream_request.insert_header("X-Proxy", "Troodon/0.1.0")?;

        // #11: Для WebSocket передаємо hop-by-hop заголовки Upgrade/Connection
        if ctx.websocket {
            if let Some(upg_val) = session.req_header().headers.get("upgrade").cloned() {
                upstream_request.insert_header("Upgrade", upg_val)?;
            }
            upstream_request.insert_header("Connection", "Upgrade")?;
            debug!(
                "WebSocket upgrade headers forwarded. ReqID={}",
                ctx.request_id
            );
        }

        // Логіка strip_prefix: якщо увімкнена, обрізаємо шлях
        if ctx.strip_prefix && !ctx.path_prefix.is_empty() && ctx.path_prefix != "/" {
            let original_path = session.req_header().uri.path();
            if let Some(stripped) = original_path.strip_prefix(&ctx.path_prefix) {
                // Запобігаємо порожньому шляху
                let new_path = if stripped.is_empty() { "/" } else { stripped };

                // Розбираємо існуючий Uri на частини (Parts) як рекомендовано Best Practices
                let mut parts = session.req_header().uri.clone().into_parts();

                let new_pq_string = match parts.path_and_query.as_ref().and_then(|pq| pq.query()) {
                    Some(query) => {
                        let mut pq = String::with_capacity(new_path.len() + 1 + query.len());
                        pq.push_str(new_path);
                        pq.push('?');
                        pq.push_str(query);
                        pq
                    }
                    None => new_path.to_string(),
                };

                if let Ok(new_pq) =
                    http::uri::PathAndQuery::from_maybe_shared(new_pq_string.clone())
                {
                    parts.path_and_query = Some(new_pq);
                    if let Ok(new_uri) = http::Uri::from_parts(parts) {
                        upstream_request.set_uri(new_uri);
                        debug!(
                            "Stripped prefix '{}'. New path: {}",
                            ctx.path_prefix, new_pq_string
                        );
                    } else {
                        error!("Invalid URI after strip_prefix: {}", new_pq_string);
                    }
                }
            }
        }

        Ok(())
    }

    // 2.5. РЕАЛЬНЕ ОБМЕЖЕННЯ РОЗМІРУ ТІЛА (захист від chunked encoding bypass)
    // Викликається для кожного chunk'а тіла запиту. Рахуємо байти і повертаємо 413
    // якщо загальний розмір перевищує ліміт. Це захищає навіть від Transfer-Encoding: chunked.
    async fn request_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<bytes::Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        if let Some(max_body) = ctx.max_body_size {
            if let Some(chunk) = body {
                ctx.body_bytes_received += chunk.len();
                if ctx.body_bytes_received > max_body {
                    warn!(
                        "🛑 Request body too large (received {} > {} bytes limit). Returning 413. ReqID={}",
                        ctx.body_bytes_received, max_body, ctx.request_id
                    );
                    // Дропаємо chunk і повертаємо помилку
                    *body = None;
                    return Err(pingora::Error::new(pingora::ErrorType::HTTPStatus(413)));
                }
            }
            if end_of_stream {
                debug!(
                    "Body stream complete: {} bytes received (limit: {}). ReqID={}",
                    ctx.body_bytes_received, max_body, ctx.request_id
                );
            }
        }
        Ok(())
    }

    // 3. RETRY ЛОГІКА (Passive Health Checks / Failover)
    // Викликається Pingora, коли не вдалося з'єднатися з upstream_peer.
    // Оскільки ми вже передали `e.set_retry(true)`, Pingora автоматично
    // повторить вибір бекенду (викличе upstream_peer наново), якщо це безпечно.
    fn fail_to_connect(
        &self,
        _session: &mut Session,
        peer: &HttpPeer,
        ctx: &mut Self::CTX,
        mut e: Box<pingora::Error>,
    ) -> Box<pingora::Error> {
        if ctx.retries_left > 0 {
            ctx.retries_left -= 1;
            e.set_retry(true);
            warn!(
                "Failed to connect to upstream {:?} (retries left: {}). Retrying...",
                peer.address(),
                ctx.retries_left
            );
        } else {
            warn!(
                "Failed to connect to upstream {:?}. No retries left, giving up.",
                peer.address()
            );
        }
        e
    }

    // 4. МОДИФІКАЦІЯ ВІДПОВІДІ (Response Header Injection)
    async fn upstream_response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Додаємо заголовок, щоб показати, що відповідь пройшла через наш проксі
        upstream_response.insert_header("X-Proxy-By", "Troodon/0.1.0")?;
        Ok(())
    }

    async fn logging(
        &self,
        session: &mut Session,
        e: Option<&pingora::Error>,
        ctx: &mut Self::CTX,
    ) {
        let response_code = session
            .response_written()
            .map(|resp| resp.status.as_u16())
            .unwrap_or(0);

        // Групуємо статус-коди (2xx, 3xx, 4xx, 5xx) для обмеження cardinality в Prometheus.
        // Без цього кожен унікальний статус-код (200, 201, 204, 301...) створює окремий time series.
        let status_class = match response_code {
            100..=199 => "1xx",
            200..=299 => "2xx",
            300..=399 => "3xx",
            400..=499 => "4xx",
            500..=599 => "5xx",
            _ => "unknown",
        };

        let method_str = session.req_header().method.as_str();

        // Host header: використовуємо effective_host, fallback до SNI
        let host_str = if !ctx.effective_host.is_empty() {
            ctx.effective_host.as_str()
        } else if !ctx.sni.is_empty() {
            ctx.sni.as_str()
        } else {
            "unknown"
        };

        // Записуємо статистику у Prometheus
        REQ_COUNTER
            .with_label_values(&[method_str, status_class, host_str])
            .inc();
        let duration = ctx.start_time.elapsed().as_secs_f64();
        REQ_DURATION
            .with_label_values(&[method_str, status_class, host_str])
            .observe(duration);

        if let Some(error) = e {
            error!(
                "Request failed: {} {} | Error: {} | Latency: {:.4}s | ReqID={}",
                session.req_header().method,
                session.req_header().uri.path(),
                error,
                duration,
                ctx.request_id
            );
        } else {
            info!(
                "ACCESS: {} {} -> {} | Latency: {:.4}s | ReqID={}",
                session.req_header().method,
                session.req_header().uri.path(),
                response_code,
                duration,
                ctx.request_id
            );
        }
    }
}
