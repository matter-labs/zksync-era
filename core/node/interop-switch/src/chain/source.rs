use std::{fmt, fmt::Debug};

use zksync_basic_types::{
    web3::{BlockNumber, FilterBuilder},
    Address, L2ChainId, H256,
};
use zksync_contracts::interop_center_contract;
use zksync_system_constants::{L2_INTEROP_CENTER_ADDRESS, L2_INTEROP_HANDLER_ADDRESS};
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::EthNamespaceClient,
};

use crate::{InteropBundle, InteropTrigger};

#[derive(Debug, Clone)]
pub struct SourceChain {
    pub(crate) chain_id: L2ChainId,
    client: Client<L2>,
    interop_center_address: Address,
    bundle_event: H256,
    trigger_event: H256,
}

impl SourceChain {
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
            interop_center_address: L2_INTEROP_CENTER_ADDRESS,
            bundle_event,
            trigger_event,
        }
    }

    pub async fn get_last_block(&self) -> anyhow::Result<u64> {
        let block = self.client.get_block_number().await?;
        Ok(block.as_u64())
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

        logs.into_iter()
            .filter_map(|log| {
                let trigger = InteropTrigger::try_from(log).unwrap();
                // if trigger.dst_chain_id != l2chain_id {
                //     return None;
                // }
                Some(trigger)
            })
            .collect()
    }

    pub async fn get_new_interop_bundles(
        &self,
        from_block: u64,
        to_block: u64,
        l2chain_id: L2ChainId,
    ) -> Vec<InteropBundle> {
        let filter = FilterBuilder::default()
            .from_block(BlockNumber::Number(from_block.into()))
            .to_block(BlockNumber::Number(to_block.into()))
            .address(vec![self.interop_center_address])
            .topics(Some(vec![self.bundle_event]), None, None, None)
            .build();
        let bundle_logs = self
            .client
            .get_logs(filter.into())
            .await
            .expect("Failed to get logs");
        bundle_logs
            .into_iter()
            .filter_map(|log| {
                let trigger = InteropBundle::try_from(log).unwrap();
                // if trigger.dst_chain_id != l2chain_id {
                //     return None;
                // }
                Some(trigger)
            })
            .collect()
    }
}
