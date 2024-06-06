use std::sync::Arc;

use anyhow::Context as _;
use tokio::{sync::watch, task::JoinHandle};
use zksync_config::{
    configs::{eth_sender::PubdataSendingMode, BaseTokenConfig},
    GasAdjusterConfig,
};
use zksync_dal::{ConnectionPool, Core};
use zksync_types::{commitment::L1BatchCommitmentMode, url::SensitiveUrl, L1ChainId};
use zksync_web3_decl::client::Client;

use crate::l1_gas_price::GasAdjuster;

/// Special struct for creating a singleton of `GasAdjuster`.
/// This is needed only for running the server.
#[derive(Debug)]
pub struct GasAdjusterSingleton {
    chain_id: L1ChainId,
    web3_url: SensitiveUrl,
    gas_adjuster_config: GasAdjusterConfig,
    pubdata_sending_mode: PubdataSendingMode,
    singleton: Option<Arc<GasAdjuster>>,
    commitment_mode: L1BatchCommitmentMode,
    connection_pool: Option<ConnectionPool<Core>>,
    base_token_config: Option<BaseTokenConfig>,
}

impl GasAdjusterSingleton {
    pub fn new(
        chain_id: L1ChainId,
        web3_url: SensitiveUrl,
        gas_adjuster_config: GasAdjusterConfig,
        pubdata_sending_mode: PubdataSendingMode,
        commitment_mode: L1BatchCommitmentMode,
        connection_pool: Option<ConnectionPool<Core>>,
        base_token_config: Option<BaseTokenConfig>,
    ) -> Self {
        Self {
            chain_id,
            web3_url,
            gas_adjuster_config,
            pubdata_sending_mode,
            singleton: None,
            commitment_mode,
            connection_pool,
            base_token_config,
        }
    }

    pub async fn get_or_init(&mut self) -> anyhow::Result<Arc<GasAdjuster>> {
        if let Some(adjuster) = &self.singleton {
            Ok(adjuster.clone())
        } else {
            let query_client = Client::http(self.web3_url.clone())
                .context("QueryClient::new()")?
                .for_network(self.chain_id.into())
                .build();
            let adjuster = GasAdjuster::new(
                Box::new(query_client),
                self.gas_adjuster_config,
                self.pubdata_sending_mode,
                self.commitment_mode,
                self.connection_pool.clone(),
                self.base_token_config.clone(),
            )
            .await
            .context("GasAdjuster::new()")?;

            self.singleton = Some(Arc::new(adjuster));
            Ok(self.singleton.as_ref().unwrap().clone())
        }
    }

    pub fn run_if_initialized(
        self,
        stop_signal: watch::Receiver<bool>,
    ) -> Option<JoinHandle<anyhow::Result<()>>> {
        let gas_adjuster = self.singleton?;
        Some(tokio::spawn(gas_adjuster.run(stop_signal)))
    }
}
