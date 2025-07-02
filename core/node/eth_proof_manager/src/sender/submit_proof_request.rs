use std::{sync::Arc, time::Duration};

use crate::client::EthProofManagerClient;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_proof_data_handler::{Locking, Processor};
use zksync_types::L2ChainId;

pub struct ProofRequestSubmitter {
    client: Box<dyn EthProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    config: EthProofManagerConfig,
    processor: Processor<Locking>,
}

impl ProofRequestSubmitter {
    pub fn new(client: Box<dyn EthProofManagerClient>, blob_store: Arc<dyn ObjectStore>, connection_pool: ConnectionPool<Core>, config: EthProofManagerConfig, proof_generation_timeout: Duration, l2_chain_id: L2ChainId) -> Self {
        let processor = Processor::<Locking>::new(blob_store.clone(), connection_pool.clone(), proof_generation_timeout, l2_chain_id);
        Self { client, connection_pool, blob_store, config, processor }
    }

    pub async fn loop_iteration(&self) -> anyhow::Result<()> {
       
    }
}