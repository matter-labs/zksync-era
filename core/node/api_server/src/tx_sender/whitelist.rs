use std::{collections::HashSet, sync::Arc, time::Duration};

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::RwLock;
use zksync_dal::transactions_dal::L2TxSubmissionResult;
use zksync_multivm::interface::{tracer::ValidationTraces, VmEvent};
use zksync_types::{h256_to_address, l2::L2Tx, Address, CONTRACT_DEPLOYER_ADDRESS};

use crate::{
    execution_sandbox::SandboxExecutionOutput,
    tx_sender::{
        master_pool_sink::MasterPoolSink, tx_sink::TxSink,
        SubmitTxError,
    },
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
        execution_outputs: &SandboxExecutionOutput,
        validation_traces: ValidationTraces,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        // Enforce the deployment allowlist by scanning for ContractDeployed events.
        // Each such event should follow this format:
        //   event ContractDeployed(address indexed deployerAddress, bytes32 indexed bytecodeHash, address indexed contractAddress);
        // We extract the deployer address from topic[1] and verify it is whitelisted.

        let deployer_addresses = execution_outputs.events.iter().filter_map(|event| {
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
            .submit_tx(tx, execution_outputs, validation_traces)
            .await
    }
}

#[derive(Debug, Clone, Default)]
pub struct SharedAllowList {
    inner: Arc<RwLock<HashSet<Address>>>,
}

impl SharedAllowList {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashSet::new())),
        }
    }
    /// Returns the internal writer, useful for updating from node_framework
    pub fn writer(&self) -> Arc<RwLock<HashSet<Address>>> {
        Arc::clone(&self.inner)
    }

    /// Checks if the given address is in the allowlist
    pub async fn is_address_allowed(&self, address: &Address) -> bool {
        self.inner.read().await.contains(address)
    }
}

#[derive(Debug, Deserialize)]
struct WhitelistResponse {
    addresses: Vec<Address>,
}


/// Task that periodically fetches and updates the allowlist from a remote HTTP source.
#[derive(Debug)]
pub struct AllowListTask {
    url: String,
    refresh_interval: Duration,
    allowlist: SharedAllowList,
    client: Client,
    etag: tokio::sync::Mutex<Option<String>>,
}

impl AllowListTask {
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
    pub fn new(url: String, refresh_interval: Duration, allowlist: SharedAllowList) -> Self {
        Self {
            url,
            refresh_interval,
            allowlist,
            client: Client::new(),
            etag: tokio::sync::Mutex::new(None),
        }
    }

    pub fn allowlist(&self) -> &SharedAllowList {
        &self.allowlist
    }

    pub fn refresh_interval(&self) -> Duration {
        self.refresh_interval
    }

    pub async fn fetch(&self) -> anyhow::Result<Option<HashSet<Address>>> {
        let etag_header = self.etag.lock().await.clone();
        let response = self
            .client
            .get(&self.url)
            .timeout(Self::REQUEST_TIMEOUT)
            .header("If-None-Match", etag_header.unwrap_or_default())
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            tracing::debug!("Allowlist unchanged (304 Not Modified)");
            return Ok(None);
        }

        let response = response.error_for_status()?;

        if let Some(etag) = response.headers().get("ETag") {
            *self.etag.lock().await = Some(etag.to_str()?.to_string());
        }

        let list = response.json::<WhitelistResponse>().await?;
        Ok(Some(list.addresses.into_iter().collect()))
    }
}