use actix_web::web;
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

    /// Creates an actix-web `Scope`, which can be mounted to the HTTP server.
    pub fn into_scope(self) -> actix_web::Scope {
        web::scope("")
            .app_data(web::Data::new(self))
            .route("/contract_verification", web::post().to(Self::verification))
            .route(
                "/contract_verification/zksolc_versions",
                web::get().to(Self::zksolc_versions),
            )
            .route(
                "/contract_verification/solc_versions",
                web::get().to(Self::solc_versions),
            )
            .route(
                "/contract_verification/zkvyper_versions",
                web::get().to(Self::zkvyper_versions),
            )
            .route(
                "/contract_verification/vyper_versions",
                web::get().to(Self::vyper_versions),
            )
            .route(
                "/contract_verification/{id}",
                web::get().to(Self::verification_request_status),
            )
            .route(
                "/contract_verification/info/{address}",
                web::get().to(Self::verification_info),
            )
    }
}
