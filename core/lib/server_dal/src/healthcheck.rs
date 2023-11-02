use serde::Serialize;
use sqlx::PgPool;

use zksync_health_check::{async_trait, CheckHealth, Health, HealthStatus};

use crate::ServerConnectionPool;

#[derive(Debug, Serialize)]
struct ServerConnectionPoolHealthDetails {
    pool_size: u32,
}

impl ServerConnectionPoolHealthDetails {
    async fn new(pool: &PgPool) -> Self {
        Self {
            pool_size: pool.size(),
        }
    }
}

// HealthCheck used to verify if we can connect to the main database.
// This guarantees that the app can use it's main "communication" channel.
// Used in the /health endpoint
#[derive(Clone, Debug)]
pub struct ServerConnectionPoolHealthCheck {
    connection_pool: ServerConnectionPool,
}

impl ServerConnectionPoolHealthCheck {
    pub fn new(connection_pool: ServerConnectionPool) -> ServerConnectionPoolHealthCheck {
        Self { connection_pool }
    }
}

#[async_trait]
impl CheckHealth for ServerConnectionPoolHealthCheck {
    fn name(&self) -> &'static str {
        "main_connection_pool"
    }

    async fn check_health(&self) -> Health {
        // This check is rather feeble, plan to make reliable here:
        // https://linear.app/matterlabs/issue/PLA-255/revamp-db-connection-health-check
        self.connection_pool.access_storage().await.unwrap();
        let details = ServerConnectionPoolHealthDetails::new(&self.connection_pool.0).await;
        Health::from(HealthStatus::Ready).with_details(details)
    }
}
