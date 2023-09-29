use zksync_config::configs::api::ExplorerApiConfig;
use zksync_dal::connection::ConnectionPool;
use zksync_types::Address;

use actix_web::web;
use futures::channel::mpsc;
use tokio::sync::watch;

use super::network_stats::SharedNetworkStats;

#[derive(Debug, Clone)]
pub struct RestApi {
    pub(super) master_connection_pool: ConnectionPool,
    pub(super) replica_connection_pool: ConnectionPool,
    pub(super) network_stats: SharedNetworkStats,
    pub(super) api_config: ExplorerApiConfig,
    pub(super) l2_erc20_bridge_addr: Address,
    pub(super) fee_account_addr: Address,
}

impl RestApi {
    pub fn new(
        master_connection_pool: ConnectionPool,
        replica_connection_pool: ConnectionPool,
        api_config: ExplorerApiConfig,
        l2_erc20_bridge_addr: Address,
        fee_account_addr: Address,
    ) -> Self {
        Self {
            master_connection_pool,
            replica_connection_pool,
            network_stats: SharedNetworkStats::default(),
            api_config,
            l2_erc20_bridge_addr,
            fee_account_addr,
        }
    }

    /// Creates an actix-web `Scope`, which can be mounted to the Http server.
    pub fn into_scope(self) -> actix_web::Scope {
        web::scope("")
            .app_data(web::Data::new(self))
            .route("/network_stats", web::get().to(Self::network_stats))
            .route("/blocks", web::get().to(Self::block_pagination))
            .route("/block/{number}", web::get().to(Self::block_details))
            .route("/l1_batches", web::get().to(Self::l1_batch_pagination))
            .route("/l1_batch/{number}", web::get().to(Self::l1_batch_details))
            .route("/transactions", web::get().to(Self::transaction_pagination))
            .route(
                "/transaction/{hash}",
                web::get().to(Self::transaction_details),
            )
            .route("/account/{address}", web::get().to(Self::account_details))
            .route("/contract/{address}", web::get().to(Self::contract_details))
            .route("/address/{address}", web::get().to(Self::address_details))
            .route("/token/{address}", web::get().to(Self::token_details))
            .route("/events", web::get().to(Self::events_pagination))
            .route(
                "/contract_verification",
                web::post().to(Self::contract_verification),
            )
            .route(
                "/contract_verification/zksolc_versions",
                web::get().to(Self::contract_verification_zksolc_versions),
            )
            .route(
                "/contract_verification/solc_versions",
                web::get().to(Self::contract_verification_solc_versions),
            )
            .route(
                "/contract_verification/zkvyper_versions",
                web::get().to(Self::contract_verification_zkvyper_versions),
            )
            .route(
                "/contract_verification/vyper_versions",
                web::get().to(Self::contract_verification_vyper_versions),
            )
            .route(
                "/contract_verification/{id}",
                web::get().to(Self::contract_verification_request_status),
            )
            .route(
                "/contract_verification/info/{address}",
                web::get().to(Self::contract_verification_info),
            )
    }

    // Spawns future updating SharedNetworkStats in the current `actix::System`
    pub fn spawn_network_stats_updater(
        &self,
        panic_notify: mpsc::Sender<bool>,
        stop_receiver: watch::Receiver<bool>,
    ) {
        self.network_stats.clone().start_updater_detached(
            panic_notify,
            self.replica_connection_pool.clone(),
            self.api_config.network_stats_interval(),
            stop_receiver,
        );
    }
}
