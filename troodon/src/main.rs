use async_trait::async_trait;
use pingora::prelude::*;
use std::sync::Arc;
use pingora::proxy::{http_proxy_service, ProxyHttp, Session};
use pingora::lb::LoadBalancer;
use pingora::lb::selection::RoundRobin;

// Наша структура, яка тримає балансувальник
pub struct LB(Arc<LoadBalancer<RoundRobin>>);

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
        let upstream = self.0.select(b"", 256).unwrap();

        println!(">> Load Balancer chose: {:?}", upstream.addr.as_inet());

        // Створюємо Peer з обраної IP
        let mut peer = Box::new(HttpPeer::new(upstream.addr, true, "one.one.one.one".to_string()));
        peer.sni = "one.one.one.one".to_string();
        // --- SECURITY BLOCK START ---
        // 1. Connection Timeout (5 сек)
        // Скільки чекаємо на TCP Handshake + TLS Handshake.
        // Якщо сервер "тупить" або лежить — кидаємо помилку, не висимо.
        peer.options.connection_timeout = Some(std::time::Duration::from_secs(5));

        // 2. Read Timeout (10 сек)
        // TTFB (Time To First Byte) та час між отриманням пакетів даних.
        // Захищає від повільних бекендів.
        peer.options.read_timeout = Some(std::time::Duration::from_secs(10));

        // 3. Write Timeout (10 сек)
        // Скільки часу ми намагаємося відправити тіло запиту на бекенд.
        peer.options.write_timeout = Some(std::time::Duration::from_secs(10));

        // 4. Idle Timeout (30 сек)
        // Це Keep-Alive. Скільки тримати з'єднання відкритим, якщо ніхто нічого не шле.
        peer.options.idle_timeout = Some(std::time::Duration::from_secs(30));
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

// ВАЖЛИВО: Додаємо tokio::main, щоб запустити асинхронний світ
//#[tokio::main]
fn main() {
    env_logger::init();

    let mut my_server = Server::new(None).unwrap();
    my_server.bootstrap();

    // Створюємо список серверів (Health Check поки немає, просто список)
    let upstreams = LoadBalancer::try_from_iter(
        ["1.1.1.1:443", 
        "1.0.0.1:443"
    ]).unwrap();

    // Ініціалізуємо сервіс
    let mut lb = http_proxy_service(&my_server.configuration, LB(Arc::new(upstreams)));
    
    lb.add_tcp("0.0.0.0:6188");

    println!("Troodon Load Balancer is active on 0.0.0.0:6188");

    my_server.add_service(lb);
    my_server.run_forever();
}