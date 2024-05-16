//! Miscellaneous helpers for the EN.

use std::time::Duration;

use futures::FutureExt;
use tokio::sync::watch;
use zksync_eth_client::EthInterface;
use zksync_health_check::{async_trait, CheckHealth, Health, HealthStatus};
use zksync_types::{L1ChainId, L2ChainId};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::ClientRpcContext,
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

/// Main node health check.
#[derive(Debug)]
pub(crate) struct MainNodeHealthCheck(Box<DynClient<L2>>);

impl From<Box<DynClient<L2>>> for MainNodeHealthCheck {
    fn from(client: Box<DynClient<L2>>) -> Self {
        Self(client.for_component("main_node_health_check"))
    }
}

#[async_trait]
impl CheckHealth for MainNodeHealthCheck {
    fn name(&self) -> &'static str {
        "main_node_http_rpc"
    }

    async fn check_health(&self) -> Health {
        if let Err(err) = self.0.get_block_number().await {
            tracing::warn!("Health-check call to main node HTTP RPC failed: {err}");
            let details = serde_json::json!({
                "error": err.to_string(),
            });
            return Health::from(HealthStatus::NotReady).with_details(details);
        }
        HealthStatus::Ready.into()
    }
}

/// Ethereum client health check.
#[derive(Debug)]
pub(crate) struct EthClientHealthCheck(Box<dyn EthInterface>);

impl From<Box<dyn EthInterface>> for EthClientHealthCheck {
    fn from(client: Box<dyn EthInterface>) -> Self {
        Self(client.for_component("ethereum_health_check"))
    }
}

#[async_trait]
impl CheckHealth for EthClientHealthCheck {
    fn name(&self) -> &'static str {
        "ethereum_http_rpc"
    }

    async fn check_health(&self) -> Health {
        if let Err(err) = self.0.block_number().await {
            tracing::warn!("Health-check call to Ethereum HTTP RPC failed: {err}");
            let details = serde_json::json!({
                "error": err.to_string(),
            });
            // Unlike main node client, losing connection to L1 is not fatal for the node
            return Health::from(HealthStatus::Affected).with_details(details);
        }
        HealthStatus::Ready.into()
    }
}

/// Task that validates chain IDs using main node and Ethereum clients.
#[derive(Debug)]
pub(crate) struct ValidateChainIdsTask {
    l1_chain_id: L1ChainId,
    l2_chain_id: L2ChainId,
    eth_client: Box<dyn EthInterface>,
    main_node_client: Box<DynClient<L2>>,
}

impl ValidateChainIdsTask {
    const BACKOFF_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(
        l1_chain_id: L1ChainId,
        l2_chain_id: L2ChainId,
        eth_client: Box<dyn EthInterface>,
        main_node_client: Box<DynClient<L2>>,
    ) -> Self {
        Self {
            l1_chain_id,
            l2_chain_id,
            eth_client: eth_client.for_component("chain_ids_validation"),
            main_node_client: main_node_client.for_component("chain_ids_validation"),
        }
    }

    async fn check_eth_client(
        eth_client: Box<dyn EthInterface>,
        expected: L1ChainId,
    ) -> anyhow::Result<()> {
        loop {
            match eth_client.fetch_chain_id().await {
                Ok(chain_id) => {
                    anyhow::ensure!(
                        expected == chain_id,
                        "Configured L1 chain ID doesn't match the one from Ethereum node. \
                        Make sure your configuration is correct and you are corrected to the right Ethereum node. \
                        Eth node chain ID: {chain_id}. Local config value: {expected}"
                    );
                    tracing::info!(
                        "Checked that L1 chain ID {chain_id} is returned by Ethereum client"
                    );
                    return Ok(());
                }
                Err(err) => {
                    tracing::warn!("Error getting L1 chain ID from Ethereum client: {err}");
                    tokio::time::sleep(Self::BACKOFF_INTERVAL).await;
                }
            }
        }
    }

    async fn check_l1_chain_using_main_node(
        main_node_client: Box<DynClient<L2>>,
        expected: L1ChainId,
    ) -> anyhow::Result<()> {
        loop {
            match main_node_client
                .l1_chain_id()
                .rpc_context("l1_chain_id")
                .await
            {
                Ok(chain_id) => {
                    let chain_id = L1ChainId(chain_id.as_u64());
                    anyhow::ensure!(
                        expected == chain_id,
                        "Configured L1 chain ID doesn't match the one from main node. \
                        Make sure your configuration is correct and you are corrected to the right main node. \
                        Main node L1 chain ID: {chain_id}. Local config value: {expected}"
                    );
                    tracing::info!(
                        "Checked that L1 chain ID {chain_id} is returned by main node client"
                    );
                    return Ok(());
                }
                Err(err) if err.is_transient() => {
                    tracing::warn!(
                        "Transient error getting L1 chain ID from main node client, will retry in {:?}: {err}",
                        Self::BACKOFF_INTERVAL
                    );
                    tokio::time::sleep(Self::BACKOFF_INTERVAL).await;
                }
                Err(err) => {
                    tracing::error!("Error getting L1 chain ID from main node client: {err}");
                    return Err(err.into());
                }
            }
        }
    }

    async fn check_l2_chain_using_main_node(
        main_node_client: Box<DynClient<L2>>,
        expected: L2ChainId,
    ) -> anyhow::Result<()> {
        loop {
            match main_node_client.chain_id().rpc_context("chain_id").await {
                Ok(chain_id) => {
                    let chain_id = L2ChainId::try_from(chain_id.as_u64()).map_err(|err| {
                        anyhow::anyhow!("invalid chain ID supplied by main node: {err}")
                    })?;
                    anyhow::ensure!(
                        expected == chain_id,
                        "Configured L2 chain ID doesn't match the one from main node. \
                        Make sure your configuration is correct and you are corrected to the right main node. \
                        Main node L2 chain ID: {chain_id:?}. Local config value: {expected:?}"
                    );
                    tracing::info!(
                        "Checked that L2 chain ID {chain_id:?} is returned by main node client"
                    );
                    return Ok(());
                }
                Err(err) if err.is_transient() => {
                    tracing::warn!(
                        "Transient error getting L2 chain ID from main node client, will retry in {:?}: {err}",
                        Self::BACKOFF_INTERVAL
                    );
                    tokio::time::sleep(Self::BACKOFF_INTERVAL).await;
                }
                Err(err) => {
                    tracing::error!("Error getting L2 chain ID from main node client: {err}");
                    return Err(err.into());
                }
            }
        }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        // Since check futures are fused, they are safe to poll after getting resolved; they will never resolve again,
        // so we'll just wait for another check or a stop signal.
        let eth_client_check = Self::check_eth_client(self.eth_client, self.l1_chain_id).fuse();
        let main_node_l1_check =
            Self::check_l1_chain_using_main_node(self.main_node_client.clone(), self.l1_chain_id)
                .fuse();
        let main_node_l2_check =
            Self::check_l2_chain_using_main_node(self.main_node_client, self.l2_chain_id).fuse();
        loop {
            tokio::select! {
                Err(err) = eth_client_check => return Err(err),
                Err(err) = main_node_l1_check => return Err(err),
                Err(err) = main_node_l2_check => return Err(err),
                _ = stop_receiver.changed() => return Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zksync_eth_client::clients::MockEthereum;
    use zksync_types::U64;
    use zksync_web3_decl::client::MockClient;

    use super::*;

    #[tokio::test]
    async fn validating_chain_ids_errors() {
        let main_node_client = MockClient::builder(L2::default())
            .method("eth_chainId", || Ok(U64::from(270)))
            .method("zks_L1ChainId", || Ok(U64::from(3)))
            .build();

        let validation_task = ValidateChainIdsTask::new(
            L1ChainId(3), // << mismatch with the Ethereum client
            L2ChainId::default(),
            Box::<MockEthereum>::default(),
            Box::new(main_node_client.clone()),
        );
        let (_stop_sender, stop_receiver) = watch::channel(false);
        let err = validation_task
            .run(stop_receiver.clone())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("L1 chain ID") && err.contains("Ethereum node"),
            "{err}"
        );

        let validation_task = ValidateChainIdsTask::new(
            L1ChainId(9), // << mismatch with the main node client
            L2ChainId::from(270),
            Box::<MockEthereum>::default(),
            Box::new(main_node_client),
        );
        let err = validation_task
            .run(stop_receiver.clone())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("L1 chain ID") && err.contains("main node"),
            "{err}"
        );

        let main_node_client = MockClient::builder(L2::default())
            .method("eth_chainId", || Ok(U64::from(270)))
            .method("zks_L1ChainId", || Ok(U64::from(9)))
            .build();

        let validation_task = ValidateChainIdsTask::new(
            L1ChainId(9),
            L2ChainId::from(271), // << mismatch with the main node client
            Box::<MockEthereum>::default(),
            Box::new(main_node_client),
        );
        let err = validation_task
            .run(stop_receiver)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("L2 chain ID") && err.contains("main node"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn validating_chain_ids_success() {
        let main_node_client = MockClient::builder(L2::default())
            .method("eth_chainId", || Ok(U64::from(270)))
            .method("zks_L1ChainId", || Ok(U64::from(9)))
            .build();

        let validation_task = ValidateChainIdsTask::new(
            L1ChainId(9),
            L2ChainId::default(),
            Box::<MockEthereum>::default(),
            Box::new(main_node_client),
        );
        let (stop_sender, stop_receiver) = watch::channel(false);
        let task = tokio::spawn(validation_task.run(stop_receiver));

        // Wait a little and ensure that the task hasn't terminated.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task.is_finished());

        stop_sender.send_replace(true);
        task.await.unwrap().unwrap();
    }
}
