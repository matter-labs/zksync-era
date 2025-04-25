use std::marker::PhantomData;

use tokio::time::Duration;
use zksync_basic_types::L2ChainId;

use crate::{DbClient, SourceChain};

pub struct InteropListener<C: DbClient> {
    src_chain: SourceChain,
    dst_chain: L2ChainId,
    listener_step: u64,
    _phantom_data: PhantomData<C>,
}

impl<C: DbClient> InteropListener<C> {
    pub fn new(src_chain: SourceChain, dst_chain: L2ChainId, listener_step: u64) -> Self {
        Self {
            src_chain,
            dst_chain,
            listener_step,
            _phantom_data: PhantomData::default(),
        }
    }

    pub async fn start(self, mut db: C) -> anyhow::Result<()> {
        loop {
            let from_block = db.get_last_processed_block(self.src_chain.chain_id).await?;
            let last_block = self.src_chain.get_last_block().await?;
            if from_block >= last_block {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            let to_block = std::cmp::min(from_block + self.listener_step, last_block);
            let bundles = self
                .src_chain
                .get_new_interop_bundles(from_block, to_block, self.dst_chain)
                .await;
            db.save_interop_bundles(bundles).await?;

            let triggered_bundles = self
                .src_chain
                .get_new_interop_triggers(from_block, to_block, self.dst_chain)
                .await;
            // TODO check the triggers for completeness
            db.save_interop_triggers(triggered_bundles).await?;
            db.update_processed_blocks(self.src_chain.chain_id, from_block, to_block)
                .await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
