use arc_swap::ArcSwap;
use async_trait::async_trait;
use matchit::Router;
use pingora::lb::LoadBalancer;
use pingora::lb::selection::RoundRobin;
use pingora::prelude::*;
use pingora::upstreams::peer::Peer;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

use lazy_static::lazy_static;
use prometheus::{
    HistogramVec, IntCounterVec, Opts, register_histogram_vec, register_int_counter_vec,
};

lazy_static! {
    static ref REQ_COUNTER: IntCounterVec = register_int_counter_vec!(
        Opts::new(
            "troodon_http_requests_total",
            "Total number of HTTP requests"
        ),
        &["method", "status", "host"]
    )
    .expect("Failed to create metric REQ_COUNTER");
    static ref REQ_DURATION: HistogramVec = register_histogram_vec!(
        "troodon_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "status", "host"]
    )
    .expect("Failed to create metric REQ_DURATION");
}

use crate::config::ServerConfig;

// === СТРУКТУРИ ===

// 1. Опис маршруту (те, що ми підготували в main.rs)
pub struct ProxyRoute {
    pub path: String,
    pub lb: Arc<LoadBalancer<RoundRobin>>,
    pub sni: String,
    pub strip_prefix: bool,
    pub max_inflight: Option<isize>,
    pub timeouts: crate::config::Timeouts,
}

// 2. Контекст запиту (наш "кошик" для передачі даних між етапами)
pub struct ProxyContext {
    pub sni: String,
    pub strip_prefix: bool,
    pub path_prefix: String,
    pub inflight_guard: Option<pingora_limits::inflight::Guard>,
    pub start_time: Instant,
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
            strip_prefix: false,
            path_prefix: String::new(),
            inflight_guard: None,
            start_time: Instant::now(),
        }
    }

    // 0. ФІЛЬТР ЗАПИТУ (Рання валідація L7 Security)
    async fn request_filter(&self, session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool> {
        // 1. Захист від Slowloris: Встановлюємо тайм-аут на читання заголовків (Client Read Timeout)
        if let Some(timeout) = self.config.client_read_timeout {
            session.set_read_timeout(Some(std::time::Duration::from_secs(timeout)));
            debug!("Set client read timeout to {}s", timeout);
        }

        // 2. Захист від OOM: Перевірка максимального розміру заголовків
        if let Some(max_size) = self.config.max_header_size {
            // Рахуємо приблизний розмір заголовків:
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
                    "🛑 Rejecting request: Headers too large ({} bytes > {} bytes max)",
                    current_size, max_size
                );
                // 431 Request Header Fields Too Large
                let _ = session.respond_error(431).await;
                return Ok(true); // Перериваємо подальшу обробку (Early return)
            }
        }

        Ok(false) // Пропускаємо запит далі
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

        info!("🔍 Trying to route path: '{}'", path); // NEW DEBUG LOG

        // Шукаємо маршрут через Radix-дерево (O(k), де k - довжина шляху)
        let route = match router_guard.routes.at(path) {
            Ok(found) => {
                info!("✅ Mathit found match!");
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
        ctx.strip_prefix = route.strip_prefix; // Чи різати?
        ctx.path_prefix = route.path.clone(); // Що різати?

        let upstream = route.lb.select(b"", 256).unwrap();
        debug!(
            "Load Balancer selected upstream: {:?}",
            upstream.addr.as_inet()
        );

        // Визначаємо, чи потрібен TLS (залежить від порту або конфігурації)
        // Поки що просто перевіряємо чи бекенд має порт 443
        let use_tls = match upstream.addr.as_inet() {
            Some(inet) => inet.port() == 443,
            None => false,
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
        if !ctx.sni.is_empty() {
            upstream_request.insert_header("Host", &ctx.sni)?;
        } else {
            error!("Context SNI is empty, something went wrong internally");
        }

        // --- ДОДАЄМО ВИЗНАЧЕННЯ КЛІЄНТА (Production Header Injection) ---
        // X-Real-IP (IP адреса TCP клієнта)
        if let Some(client_ip) = session.client_addr() {
            if let Some(ip) = client_ip.as_inet() {
                let ip_str = ip.ip().to_string();
                upstream_request.insert_header("X-Real-IP", &ip_str)?;
                upstream_request.insert_header("X-Forwarded-For", &ip_str)?;
            }
        }

        // Можна додати X-Request-Id (для трейсингу)
        upstream_request.insert_header("X-Proxy", "Troodon/0.1.0")?;
        // ----------------------------------------------------------------

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

    // 3. RETRY ЛОГІКА (Passive Health Checks / Failover)
    // Викликається Pingora, коли не вдалося з'єднатися з upstream_peer.
    // Оскільки ми вже передали `e.set_retry(true)`, Pingora автоматично
    // повторить вибір бекенду (викличе upstream_peer наново), якщо це безпечно.
    fn fail_to_connect(
        &self,
        _session: &mut Session,
        peer: &HttpPeer,
        _ctx: &mut Self::CTX,
        mut e: Box<pingora::Error>,
    ) -> Box<pingora::Error> {
        e.set_retry(true);
        warn!(
            "Failed to connect to upstream {:?}, error: {}. Retrying...",
            peer.address(),
            e
        );
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

        let status_str = response_code.to_string();
        let method_str = session.req_header().method.as_str();

        // Host header can be empty, fallback to unknown
        let host_str = if ctx.sni.is_empty() {
            "unknown"
        } else {
            &ctx.sni
        };

        // Записуємо статистику у Prometheus
        REQ_COUNTER
            .with_label_values(&[method_str, &status_str, host_str])
            .inc();
        let duration = ctx.start_time.elapsed().as_secs_f64();
        REQ_DURATION
            .with_label_values(&[method_str, &status_str, host_str])
            .observe(duration);

        if let Some(error) = e {
            error!(
                "Request failed: {} {} | Error: {} | Latency: {:.4}s",
                session.req_header().method,
                session.req_header().uri.path(),
                error,
                duration
            );
        } else {
            info!(
                "ACCESS: {} {} -> {} | Latency: {:.4}s",
                session.req_header().method,
                session.req_header().uri.path(),
                response_code,
                duration
            );
        }
    }
}
