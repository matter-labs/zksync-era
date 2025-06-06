use std::sync::Arc;

use tower_http::cors::CorsLayer;
use zksync_dal::{ConnectionPool, Core};

use crate::cache::SupportedCompilersCache;

#[derive(Debug, Clone)]
pub(crate) struct RestApi {
    pub(crate) master_connection_pool: ConnectionPool<Core>,
    pub(crate) replica_connection_pool: ConnectionPool<Core>,
    pub(crate) supported_compilers: Arc<SupportedCompilersCache>,
}

impl RestApi {
    pub fn new(
        master_connection_pool: ConnectionPool<Core>,
        replica_connection_pool: ConnectionPool<Core>,
    ) -> Self {
        let supported_compilers = SupportedCompilersCache::new(replica_connection_pool.clone());
        Self {
            supported_compilers: Arc::new(supported_compilers),
            master_connection_pool,
            replica_connection_pool,
        }
    }

    pub fn into_router(self) -> axum::Router<()> {
        axum::Router::new()
            .route(
                "/contract_verification",
                axum::routing::get(Self::etherscan_get_action),
            )
            .route("/contract_verification", axum::routing::post(Self::post))
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
            .layer(CorsLayer::permissive())
            .with_state(Arc::new(self))
    }
}
