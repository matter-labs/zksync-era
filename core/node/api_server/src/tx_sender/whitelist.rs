use std::{collections::HashSet, sync::Arc, time::Duration};

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::{watch, RwLock};
use zksync_config::configs::api::DeploymentAllowlist;
use zksync_dal::transactions_dal::L2TxSubmissionResult;
use zksync_multivm::interface::{tracer::ValidationTraces, VmEvent};
use zksync_types::{h256_to_address, l2::L2Tx, Address, CONTRACT_DEPLOYER_ADDRESS};

use crate::{
    execution_sandbox::SandboxExecutionOutput,
    tx_sender::{master_pool_sink::MasterPoolSink, tx_sink::TxSink, SubmitTxError},
};

/// Wrapper that submits transactions to the mempool and enforces contract deployment allow-list
/// by analyzing the VM events for contract deployments.
#[derive(Debug)]
pub struct WhitelistedDeployPoolSink {
    master_pool_sink: MasterPoolSink,
    shared_allow_list: SharedAllowList,
}

impl WhitelistedDeployPoolSink {
    pub fn new(master_pool_sink: MasterPoolSink, shared_allow_list: SharedAllowList) -> Self {
        Self {
            master_pool_sink,
            shared_allow_list,
        }
    }
}

#[async_trait::async_trait]
impl TxSink for WhitelistedDeployPoolSink {
    async fn submit_tx(
        &self,
        tx: &L2Tx,
        execution_output: &SandboxExecutionOutput,
        validation_traces: ValidationTraces,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        // Enforce the deployment allowlist by scanning for ContractDeployed events.
        // Each such event should follow this format:
        //   event ContractDeployed(address indexed deployerAddress, bytes32 indexed bytecodeHash, address indexed contractAddress);
        // We extract the deployer address from topic[1] and verify it is whitelisted.

        let deployer_addresses = execution_output.events.iter().filter_map(|event| {
            let is_contract_deployed = event.address == CONTRACT_DEPLOYER_ADDRESS
                && event.indexed_topics.first() == Some(&VmEvent::DEPLOY_EVENT_SIGNATURE);
            if is_contract_deployed {
                event.indexed_topics.get(1).map(h256_to_address)
            } else {
                None
            }
        });

        for deployer_address in deployer_addresses {
            if !self
                .shared_allow_list
                .is_address_allowed(&deployer_address)
                .await
            {
                tracing::info!(
                    "Blocking contract deployment in tx {:?}: deployer_address {:?} not whitelisted",
                    tx.hash(),
                    deployer_address
                );
                return Err(SubmitTxError::DeployerNotInAllowList(deployer_address));
            }
        }

        // If all deployment events pass the allowlist check, forward the submission.
        self.master_pool_sink
            .submit_tx(tx, execution_output, validation_traces)
            .await
    }
}

#[derive(Debug, Deserialize)]
struct WhitelistResponse {
    addresses: Vec<Address>,
}

#[derive(Debug, Clone, Default)]
pub struct SharedAllowList {
    inner: Arc<RwLock<HashSet<Address>>>,
}

impl SharedAllowList {
    fn writer(&self) -> &Arc<RwLock<HashSet<Address>>> {
        &self.inner
    }

    async fn is_address_allowed(&self, address: &Address) -> bool {
        self.inner.read().await.contains(address)
    }
}

/// Task that periodically fetches and updates the allowlist from a remote HTTP source.
#[derive(Debug, Clone)]
pub struct AllowListTask {
    url: String,
    refresh_interval: Duration,
    allowlist: SharedAllowList,
    client: Client,
}

impl AllowListTask {
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
    pub fn from_config(deployment_allowlist: DeploymentAllowlist) -> Self {
        Self {
            url: deployment_allowlist
                .http_file_url()
                .expect("DeploymentAllowlist must contain a URL")
                .to_string(),
            refresh_interval: deployment_allowlist.refresh_interval(),
            allowlist: SharedAllowList::default(),
            client: Client::new(),
        }
    }

    fn refresh_interval(&self) -> Duration {
        self.refresh_interval
    }

    pub fn shared(&self) -> SharedAllowList {
        self.allowlist.clone()
    }

    async fn fetch(
        &self,
        current_etag: Option<&str>,
    ) -> anyhow::Result<Option<(HashSet<Address>, Option<String>)>> {
        let mut request = self.client.get(&self.url).timeout(Self::REQUEST_TIMEOUT);

        if let Some(etag) = current_etag {
            request = request.header("If-None-Match", etag);
        }

        let response = request.send().await?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            tracing::debug!("Allowlist unchanged (304 Not Modified)");
            return Ok(None);
        }

        let response = response.error_for_status()?;

        let new_etag = match response.headers().get("ETag") {
            Some(value) => match value.to_str() {
                Ok(s) => Some(s.to_string()),
                Err(_) => None,
            },
            None => None,
        };
        let list = response.json::<WhitelistResponse>().await?;
        let addresses: HashSet<_> = list.addresses.into_iter().collect();

        Ok(Some((addresses, new_etag)))
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut etag: Option<String> = None;

        while !*stop_receiver.borrow_and_update() {
            match self.fetch(etag.as_deref()).await {
                Ok(Some((new_list, new_etag))) => {
                    let writer = self.allowlist.writer();
                    let mut lock = writer.write().await;

                    *lock = new_list;
                    etag = new_etag;
                    tracing::debug!("Allowlist updated. {} entries loaded.", lock.len());
                }
                Ok(None) => {
                    tracing::debug!("Allowlist not updated (ETag matched).");
                }
                Err(err) => {
                    tracing::warn!("Failed to refresh allowlist: {}", err);
                }
            }
            let _ = tokio::time::timeout(self.refresh_interval(), stop_receiver.changed()).await;
        }

        tracing::info!("received a stop signal; allow list task is shut down");

        Ok(())
    }
}
