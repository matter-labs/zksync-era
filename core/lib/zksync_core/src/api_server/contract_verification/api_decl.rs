use std::sync::Arc;

use zksync_dal::{ConnectionPool, Core};

#[derive(Debug, Clone)]
pub struct RestApi {
    pub(super) master_connection_pool: ConnectionPool<Core>,
    pub(super) replica_connection_pool: ConnectionPool<Core>,
}

impl RestApi {
    pub fn new(
        master_connection_pool: ConnectionPool<Core>,
        replica_connection_pool: ConnectionPool<Core>,
    ) -> Self {
        Self {
            master_connection_pool,
            replica_connection_pool,
        }
    }

    pub fn into_router(self) -> axum::Router<()> {
        axum::Router::new()
            .route(
                "/contract_verification",
                axum::routing::post(Self::verification),
            )
            .route(
                "/contract_verification/zksolc_versions",
                axum::routing::get(Self::zksolc_versions),
            )
            .route(
                "/contract_verification/solc_versions",
                axum::routing::get(Self::solc_versions),
            )
            .route(
                "/contract_verification/zkvyper_versions",
                axum::routing::get(Self::zkvyper_versions),
            )
            .route(
                "/contract_verification/vyper_versions",
                axum::routing::get(Self::vyper_versions),
            )
            .route(
                "/contract_verification/:id",
                axum::routing::get(Self::verification_request_status),
            )
            .route(
                "/contract_verification/info/:address",
                axum::routing::get(Self::verification_info),
            )
            .with_state(Arc::new(self))
    }
}
