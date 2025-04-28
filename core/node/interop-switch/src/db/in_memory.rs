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
    processed_blocks: HashMap<L2ChainId, u64>,
}

#[async_trait::async_trait]
impl DbClient for InMemoryDb {
    async fn save_interop_trigger(&mut self, tx: InteropTrigger) -> anyhow::Result<()> {
        self.triggers.insert(tx.tx_hash, tx);
        Ok(())
    }

    async fn save_interop_bundle(&mut self, tx: InteropBundle) -> anyhow::Result<()> {
        self.bundles.insert(tx.bundle_hash, tx);
        Ok(())
    }

    async fn get_interop_bundle(&mut self, tx_hash: H256) -> anyhow::Result<Option<InteropBundle>> {
        Ok(self.bundles.get(&tx_hash).cloned())
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
        _from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<()> {
        self.processed_blocks.insert(src_chain_id, to_block);
        Ok(())
    }

    async fn get_last_processed_block(&mut self, src_chain_id: L2ChainId) -> anyhow::Result<u64> {
        Ok(self
            .processed_blocks
            .get(&src_chain_id)
            .cloned()
            .unwrap_or_default())
    }
}
