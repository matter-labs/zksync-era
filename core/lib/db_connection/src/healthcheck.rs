use serde::Serialize;
use zksync_health_check::{async_trait, CheckHealth, Health, HealthStatus};

use crate::{connection::DbMarker, connection_pool::ConnectionPool};

#[derive(Debug, Serialize)]
struct ConnectionPoolHealthDetails {
    pool_size: u32,
    max_size: u32,
}

impl ConnectionPoolHealthDetails {
    fn new<DB: DbMarker>(pool: &ConnectionPool<DB>) -> Self {
        Self {
            pool_size: pool.inner.size(),
            max_size: pool.max_size(),
        }
    }
}

// HealthCheck used to verify if we can connect to the database.
// This guarantees that the app can use it's main "communication" channel.
// Used in the /health endpoint
#[derive(Clone, Debug)]
pub struct ConnectionPoolHealthCheck<DB: DbMarker> {
    connection_pool: ConnectionPool<DB>,
}

impl<DB: DbMarker> ConnectionPoolHealthCheck<DB> {
    pub fn new(connection_pool: ConnectionPool<DB>) -> ConnectionPoolHealthCheck<DB> {
        Self { connection_pool }
    }
}

#[async_trait]
impl<DB: DbMarker + Send + Sync + 'static> CheckHealth for ConnectionPoolHealthCheck<DB> {
    fn name(&self) -> &'static str {
        "connection_pool"
    }

    async fn check_health(&self) -> Health {
        // This check is rather feeble, plan to make reliable here:
        // https://linear.app/matterlabs/issue/PLA-255/revamp-db-connection-health-check
        match self.connection_pool.connection().await {
            Ok(_) => {
                let details = ConnectionPoolHealthDetails::new(&self.connection_pool);
                Health::from(HealthStatus::Ready).with_details(details)
            }
            Err(err) => {
                tracing::warn!("Failed acquiring DB connection for health check: {err:?}");
                let details = serde_json::json!({
                    "error": format!("{err:?}"),
                });
                Health::from(HealthStatus::NotReady).with_details(details)
            }
        }
    }
}
