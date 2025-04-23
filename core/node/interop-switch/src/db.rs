use std::collections::HashMap;

use zksync_basic_types::{L2ChainId, H256};

use crate::{
    types::{InteropBundle, InteropTrigger},
    DbClient,
};

/// In-memory database for interop transactions.
/// This is a simple implementation of the `DbClient` trait that stores
/// the transactions in memory.
#[derive(Debug, Clone, Default)]
pub struct InMemoryDb {
    bundles: HashMap<H256, InteropBundle>,
    triggers: HashMap<H256, InteropTrigger>,
}

#[async_trait::async_trait]
impl DbClient for InMemoryDb {
    async fn save_interop_trigger(&mut self, tx: InteropTrigger) -> anyhow::Result<()> {
        todo!()
    }

    async fn save_interop_triggers(&mut self, txs: Vec<InteropTrigger>) -> anyhow::Result<()> {
        todo!()
    }

    async fn save_interop_bundle(&mut self, tx: InteropBundle) -> anyhow::Result<()> {
        todo!()
    }

    async fn save_interop_bundles(&mut self, txs: Vec<InteropBundle>) -> anyhow::Result<()> {
        todo!()
    }

    async fn get_interop_bundle(&mut self, tx_hash: H256) -> anyhow::Result<InteropBundle> {
        todo!()
    }

    async fn get_interop_tx(&mut self, tx_hash: H256) -> anyhow::Result<()> {
        todo!()
    }

    async fn commit_interop_tx(&mut self, tx_hash: H256) -> anyhow::Result<()> {
        todo!()
    }

    async fn update_processed_blocks(
        &mut self,
        src_chain_id: L2ChainId,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<()> {
        todo!()
    }

    async fn get_last_processed_block(&mut self, src_chain_id: L2ChainId) -> anyhow::Result<u64> {
        todo!()
    }

    async fn inject_new_fee_bundle(
        &mut self,
        tx_hash: H256,
        fee_bundle: Vec<u8>,
    ) -> anyhow::Result<()> {
        todo!()
    }
}
