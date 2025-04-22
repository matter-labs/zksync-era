use zksync_basic_types::ethabi::Contract;
use zksync_basic_types::{Address, L2ChainId};
use zksync_contracts::interop_center_contract;
use zksync_system_constants::L2_INTEROP_HANDLER_ADDRESS;
use zksync_web3_decl::client::{Client, L2};

use crate::{InteropBundle, InteropTrigger};

#[derive(Debug, Clone)]
pub struct Chain {
    pub(crate) chain_id: L2ChainId,
    client: Client<L2>,
    introp_center_address: Address,
    interop_contract: Contract,
}

impl Chain {
    pub async fn new(client: Client<L2>) -> Self {
        // TODO handle error
        let chain_id = client.chain_id().await.expect("Failed to get chain ID");
        Self {
            chain_id: L2ChainId::new(chain_id.as_u64()).expect("Invalid chain ID from client"),
            client,
            introp_center_address: L2_INTEROP_HANDLER_ADDRESS,
            interop_contract: interop_center_contract(),
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
