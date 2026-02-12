use async_trait::async_trait;
use pingora::prelude::*;
use pingora::lb::LoadBalancer;
use pingora::lb::selection::RoundRobin;
use std::sync::Arc;
use tracing::{debug, info, error, warn};
use crate::config::ServerConfig;

// === СТРУКТУРИ ===

// 1. Опис маршруту (те, що ми підготували в main.rs)
pub struct ProxyRoute {
    pub path: String,
    pub lb: Arc<LoadBalancer<RoundRobin>>,
    pub sni: String,
}

// 2. Контекст запиту (наш "кошик" для передачі даних між етапами)
pub struct ProxyContext {
    pub sni: String, // Тут ми будемо зберігати SNI, який знайшли в upstream_peer
    pub strip_prefix: bool,
    // Що саме обрізати? (наприклад "/api")
    pub path_prefix: String,
}

// 3. Головна структура
pub struct LB {
    pub routes: Arc<Vec<ProxyRoute>>, 
    pub config: Arc<ServerConfig>,
}

#[async_trait]
impl ProxyHttp for LB {
    // === ВАЖЛИВО: Визначаємо наш тип контексту ===
    type CTX = ProxyContext;
    
    // Ініціалізуємо контекст порожнім рядком
    fn new_ctx(&self) -> Self::CTX {
        ProxyContext { 
            sni: String::new(),
            strip_prefix: false,
            path_prefix: String::new(),
        }
    }

    // 1. ВИБІР БЕКЕНДУ
    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX, // Отримуємо доступ до контексту
    ) -> Result<Box<HttpPeer>> {
        
        let path = session.req_header().uri.path();
        
        // Шукаємо маршрут (Longest Prefix Match - поки беремо перший підходящий)
        let matching_route = self.routes.iter()
            .find(|route| path.starts_with(&route.path));

        let route = match matching_route {
            Some(r) => r,
            None => {
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
        ctx.path_prefix = route.path.clone();  // Що різати?

        let upstream = route.lb.select(b"", 256).unwrap();
        debug!("Load Balancer selected upstream: {:?}", upstream.addr.as_inet());

        let mut peer = Box::new(HttpPeer::new(
            upstream.addr, 
            true, 
            route.sni.clone() 
        ));
        
        peer.sni = route.sni.clone();
        
        // Тайм-аути
        let timeouts = &self.config.timeouts;
        peer.options.connection_timeout = Some(std::time::Duration::from_secs(timeouts.connect));
        peer.options.read_timeout = Some(std::time::Duration::from_secs(timeouts.read));
        peer.options.write_timeout = Some(std::time::Duration::from_secs(timeouts.write));
        peer.options.idle_timeout = Some(std::time::Duration::from_secs(timeouts.idle));

        Ok(peer)
    }

    // 2. МОДИФІКАЦІЯ ЗАГОЛОВКІВ
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX, // Читаємо контекст
    ) -> Result<()> {
        // Більше ніяких пошуків і хардкоду!
        // Ми просто беремо те, що поклали в upstream_peer
        if !ctx.sni.is_empty() {
            upstream_request.insert_header("Host", &ctx.sni).unwrap();
        } else {
            // Це станеться тільки якщо upstream_peer повернув помилку,
            // але тоді цей метод і не викличеться.
            error!("Context SNI is empty, something went wrong internally");
        }

        Ok(())
    }

    async fn logging(
        &self,
        session: &mut Session,
        e: Option<&pingora::Error>,
        _ctx: &mut Self::CTX,
    ) {
        let response_code = session
            .response_written()
            .map(|resp| resp.status.as_u16())
            .unwrap_or(0);

        if let Some(error) = e {
            error!(
                "Request failed: {} {} | Error: {}", 
                session.req_header().method, 
                session.req_header().uri.path(),
                error
            );
        } else {
            info!(
                "ACCESS: {} {} -> {}",
                session.req_header().method,
                session.req_header().uri.path(),
                response_code
            );
        }
    }
}