use serde::Serialize;
use sqlx::PgPool;

use zksync_db_connection::ConnectionPool;
use zksync_health_check::{async_trait, CheckHealth, Health, HealthStatus};

use crate::ServerStorageProcessor;

#[derive(Debug, Serialize)]
struct ConnectionPoolHealthDetails {
    pool_size: u32,
}

impl ConnectionPoolHealthDetails {
    async fn new(pool: &PgPool) -> Self {
        Self {
            pool_size: pool.size(),
        }
    }
}

// HealthCheck used to verify if we can connect to the main database.
// This guarantees that the app can use it's main "communication" channel.
// Used in the /health endpoint
#[derive(Debug, Clone)]
pub struct ServerConnectionPoolHealthCheck {
    connection_pool: ConnectionPool,
}

impl<'a> ServerConnectionPoolHealthCheck {
    pub fn new(connection_pool: ConnectionPool) -> ServerConnectionPoolHealthCheck {
        Self { connection_pool }
    }
}

#[async_trait]
impl CheckHealth for ServerConnectionPoolHealthCheck {
    fn name(&self) -> &'static str {
        "server_connection_pool"
    }

    async fn check_health(&self) -> Health {
        // This check is rather feeble, plan to make reliable here:
        // https://linear.app/matterlabs/issue/PLA-255/revamp-db-connection-health-check
        self.connection_pool
            .access_storage::<ServerStorageProcessor>()
            .await
            .unwrap();
        let details = ConnectionPoolHealthDetails::new(&self.connection_pool.0).await;
        Health::from(HealthStatus::Ready).with_details(details)
    }
}
