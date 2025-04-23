use zksync_basic_types::ethabi::Event;
use zksync_basic_types::{
    ethabi::Contract,
    web3::{BlockNumber, FilterBuilder},
    Address, L2ChainId, H256,
};
use zksync_contracts::interop_center_contract;
use zksync_system_constants::L2_INTEROP_HANDLER_ADDRESS;
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::EthNamespaceClient,
    types::Filter,
};

use crate::{InteropBundle, InteropTrigger};

#[derive(Debug, Clone)]
pub struct Chain {
    pub(crate) chain_id: L2ChainId,
    client: Client<L2>,
    interop_center_address: Address,
    bundle_event: H256,
    trigger_event: H256,
}

impl Chain {
    pub async fn new(client: Client<L2>) -> Self {
        // TODO handle error
        let chain_id = client.chain_id().await.expect("Failed to get chain ID");
        let contact = interop_center_contract();
        let bundle_event = contact
            .event("InteropBundleSent")
            .expect("Failed to get InteropBundleSent event")
            .signature();
        let trigger_event = contact
            .event("InteropTriggerSent")
            .expect("Failed to get InteropTriggerSent event")
            .signature();
        Self {
            chain_id: L2ChainId::new(chain_id.as_u64()).expect("Invalid chain ID from client"),
            client,
            interop_center_address: L2_INTEROP_HANDLER_ADDRESS,
            bundle_event,
            trigger_event,
        }
    }

    pub async fn get_new_interop_triggers(
        &self,
        from_block: u64,
        to_block: u64,
        l2chain_id: L2ChainId,
    ) -> Vec<InteropTrigger> {
        let filter = FilterBuilder::default()
            .from_block(BlockNumber::Number(from_block.into()))
            .to_block(BlockNumber::Number(to_block.into()))
            .address(vec![self.interop_center_address])
            .topics(Some(vec![self.trigger_event]), None, None, None)
            .build();
        let logs = self
            .client
            .get_logs(filter.into())
            .await
            .expect("Failed to get logs");

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
