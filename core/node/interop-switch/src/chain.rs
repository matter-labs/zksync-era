use zksync_basic_types::L2ChainId;
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::EthNamespaceClient,
};

use crate::{InteropBundle, InteropTrigger};

#[derive(Debug, Clone)]
pub struct Chain {
    pub chain_id: L2ChainId,
    pub client: Client<L2>,
}

impl Chain {
    pub async fn new(client: Client<L2>) -> Self {
        // TODO handle error
        let chain_id = client.chain_id().await.expect("Failed to get chain ID");
        Self {
            chain_id: L2ChainId::new(chain_id.as_u64()).expect("Invalid chain ID from client"),
            client,
        }
    }

    pub async fn get_new_interop_triggers(
        &self,
        from_block: u64,
        to_block: u64,
        l2chain_id: L2ChainId,
    ) -> Vec<InteropTrigger> {
        vec![]
    }

    pub async fn get_new_interop_bundles(
        &self,
        from_block: u64,
        to_block: u64,
        l2chain_id: L2ChainId,
    ) -> Vec<InteropBundle> {
        vec![]
    }
}
