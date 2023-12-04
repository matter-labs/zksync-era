use serde::Serialize;
use sqlx::PgPool;

use zksync_db_connection::ConnectionPool;
use zksync_health_check::{async_trait, CheckHealth, Health, HealthStatus};

use crate::ProverStorageProcessor;

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

// HealthCheck used to verify if we can connect to the prover database.
// This guarantees that the app can use it's main "communication" channel.
// Used in the /health endpoint
#[derive(Debug, Clone)]
pub struct ProverConnectionPoolHealthCheck {
    connection_pool: ConnectionPool,
}

impl ProverConnectionPoolHealthCheck {
    pub fn new(connection_pool: ConnectionPool) -> ProverConnectionPoolHealthCheck {
        Self { connection_pool }
    }
}

#[async_trait]
impl CheckHealth for ProverConnectionPoolHealthCheck {
    fn name(&self) -> &'static str {
        "prover_connection_pool"
    }

    async fn check_health(&self) -> Health {
        // This check is rather feeble, plan to make reliable here:
        // https://linear.app/matterlabs/issue/PLA-255/revamp-db-connection-health-check
        self.connection_pool
            .access_storage::<ProverStorageProcessor>()
            .await
            .unwrap();
        let details = ConnectionPoolHealthDetails::new(&self.connection_pool.0).await;
        Health::from(HealthStatus::Ready).with_details(details)
    }
}
