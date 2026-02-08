use async_trait::async_trait;
use pingora::prelude::*;
use pingora::lb::LoadBalancer;
use pingora::lb::selection::RoundRobin;
use std::sync::Arc;
use tracing::{debug, info, error}; // info! для access logs
use crate::config::ServerConfig;

// Наша структура-балансувальник
// Pub, щоб main міг її бачити
//pub struct LB(pub Arc<LoadBalancer<RoundRobin>>);

pub struct LB {
    pub upstreams:  Arc<LoadBalancer<RoundRobin>>,
    pub config: Arc<ServerConfig>, // <--- Зберігаємо конфіг тут
}


#[async_trait]
impl ProxyHttp for LB {
    // 1. Визначаємо тип контексту (обов'язково!)
    type CTX = ();
    
    // 2. Створюємо контекст (обов'язково!)
    fn new_ctx(&self) -> Self::CTX {
        ()
    }

    // 3. Визначаємо, куди слати трафік (обов'язково!)
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        // ВИКОРИСТОВУЄМО БАЛАНСУВАЛЬНИК
        // self.0 — це доступ до першого елементу нашої структури (Arc<LoadBalancer>)
        // select() обирає бекенд за алгоритмом RoundRobin
        let upstream = self.upstreams.select(b"", 256).unwrap();

        debug!("Load Balancer selected upstream: {:?}", upstream.addr.as_inet());

        // Створюємо Peer з обраної IP
        let mut peer = Box::new(HttpPeer::new(upstream.addr, true, "one.one.one.one".to_string()));
        peer.sni = "one.one.one.one".to_string();
        // --- SECURITY BLOCK START ---
        // 1. Connection Timeout (5 сек)
        // Скільки чекаємо на TCP Handshake + TLS Handshake.
        // Якщо сервер "тупить" або лежить — кидаємо помилку, не висимо.
        let timeouts = &self.config.timeouts;

        // 1. Connection Timeout (Connect)
        peer.options.connection_timeout = Some(std::time::Duration::from_secs(timeouts.connect));

        // 2. Read Timeout (TTFB)
        peer.options.read_timeout = Some(std::time::Duration::from_secs(timeouts.read));

        // 3. Write Timeout
        peer.options.write_timeout = Some(std::time::Duration::from_secs(timeouts.write));

        // 4. Idle Timeout (Keep-Alive)
        peer.options.idle_timeout = Some(std::time::Duration::from_secs(timeouts.idle));
        // --- SECURITY BLOCK END ---

        Ok(peer)
    }

    // 4. Фільтр заголовків
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        upstream_request.insert_header("Host", "one.one.one.one").unwrap();
        Ok(())
    }
}