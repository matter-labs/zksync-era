use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_eth_client::{clients::L1, DynClient};
use zksync_object_store::ObjectStore;
use zksync_proof_data_handler::processor::{Processor, Readonly};
use zksync_prover_interface::inputs::WitnessInputData;
use zksync_types::{Address, Contract, L2ChainId};

use crate::{
    client::EthProofManagerClient, readiness_checker::ReadinessChecker, sender::EthProofSender,
    watcher::EthProofWatcher,
};

mod client;
mod metrics;
pub mod node;
pub mod readiness_checker;
pub mod sender;
mod types;
pub mod watcher;

pub struct EthProofManager {
    sender: EthProofSender,
    watcher: EthProofWatcher,
    readiness_checker: ReadinessChecker,
}

impl EthProofManager {
    pub fn new(
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        client: Box<DynClient<L1>>,
        proof_manager_address: Address,
        proof_manager_abi: Contract,
        proof_data_handler_config: ProofDataHandlerConfig,
        chain_id: L2ChainId,
    ) -> Self {
        let client = EthProofManagerClient::new(client, proof_manager_address, proof_manager_abi);
        let processor = Processor::new(
            connection_pool.clone(),
            blob_store.clone(),
            proof_data_handler_config,
            chain_id,
        );

        let sender = EthProofSender::new(
            chain_id,
            client,
            connection_pool.clone(),
            blob_store.clone(),
        );
        let watcher = EthProofWatcher::new(
            client,
            connection_pool.clone(),
            blob_store.clone(),
            Duration::from_secs(10),
        );
        let readiness_checker = ReadinessChecker::new(
            processor,
            chain_id,
            connection_pool.clone(),
            blob_store.clone(),
        );
        Self {
            sender,
            watcher,
            readiness_checker,
        }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tokio::select! {
             _ = self.sender.run(stop_receiver.clone()) => {
                 Ok(())
             }
             _ = self.watcher.run(stop_receiver.clone()) => {
                 Ok(())
             }
             _ = self.readiness_checker.run(stop_receiver.clone()) => {
                 Ok(())
             }
        }
    }
}
