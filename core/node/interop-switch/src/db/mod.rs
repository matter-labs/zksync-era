use zksync_basic_types::{L2ChainId, H256};

use crate::types::{InteropBundle, InteropTrigger};
pub(crate) mod in_memory;

#[async_trait::async_trait]
pub trait DbClient: Clone {
    async fn save_interop_trigger(&mut self, tx: InteropTrigger) -> anyhow::Result<()>;

    async fn save_interop_triggers(&mut self, txs: Vec<InteropTrigger>) -> anyhow::Result<()> {
        for tx in txs {
            self.save_interop_trigger(tx).await?;
        }
        Ok(())
    }
    async fn save_interop_bundle(&mut self, tx: InteropBundle) -> anyhow::Result<()>;

    async fn save_interop_bundles(&mut self, txs: Vec<InteropBundle>) -> anyhow::Result<()> {
        for tx in txs {
            self.save_interop_bundle(tx).await?;
        }
        Ok(())
    }

    async fn get_interop_bundle(&mut self, tx_hash: H256) -> anyhow::Result<Option<InteropBundle>>;
    async fn get_interop_tx(&mut self, tx_hash: H256) -> anyhow::Result<()>;
    async fn commit_interop_tx(&mut self, tx_hash: H256) -> anyhow::Result<()>;
    async fn update_processed_blocks(
        &mut self,
        src_chain_id: L2ChainId,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<()>;

    async fn get_last_processed_block(&mut self, src_chain_id: L2ChainId) -> anyhow::Result<u64>;

    async fn inject_new_fee_bundle(
        &mut self,
        tx_hash: H256,
        fee_bundle: Vec<u8>,
    ) -> anyhow::Result<()>;
}
