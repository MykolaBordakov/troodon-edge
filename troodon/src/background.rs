use crate::proxy::ProxyRouter;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use pingora::services::background::BackgroundService;
use std::sync::Arc;
use tracing::{debug, info};

pub struct RouterHealthCheck {
    pub router: Arc<ArcSwap<ProxyRouter>>,
}

#[async_trait]
impl BackgroundService for RouterHealthCheck {
    async fn start(&self, mut shutdown: pingora::server::ShutdownWatch) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let current_router = self.router.load();
                    for (path, lb) in &current_router.health_checks {
                        debug!("Running Health Check for {}", path);
                        lb.backends().run_health_check(true).await;
                    }
                }
                _ = shutdown.changed() => {
                    info!("Health Check Service shutting down.");
                    break;
                }
            }
        }
    }
}
