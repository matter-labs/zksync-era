use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use zksync_basic_types::{Address, L2ChainId, H256, U256};

use crate::{chain::Chain, listener::InteropListener};

mod chain;
mod listener;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteropCall {
    pub direct_call: bool,
    pub to: Address,
    pub from: Address,
    pub value: U256,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct InteropTrigger {
    pub tx_hash: H256,
    pub src_chain_id: L2ChainId,
    pub dst_chain_id: L2ChainId,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct InteropBundle {
    pub tx_hash: H256,
    pub src_chain_id: L2ChainId,
    pub dst_chain_id: L2ChainId,
    pub calls: Vec<InteropCall>,
    pub execution_address: Address,
}

#[async_trait::async_trait]
pub trait DbClient {
    async fn save_interop_trigger(&mut self, tx: InteropTrigger) -> Result<(), String>;
    async fn save_interop_triggers(&mut self, txs: Vec<InteropTrigger>) -> Result<(), String>;
    async fn save_interop_bundle(&mut self, tx: InteropBundle) -> Result<(), String>;
    async fn save_interop_bundles(&mut self, txs: Vec<InteropBundle>) -> Result<(), String>;
    async fn get_interop_bundle(&mut self, tx_hash: H256) -> Result<InteropBundle, String>;
    async fn get_interop_tx(&mut self, tx_hash: H256) -> Result<(), String>;
    async fn commit_interop_tx(&mut self, tx_hash: H256) -> Result<(), String>;
    async fn update_processed_blocks(
        &mut self,
        src_chain_id: L2ChainId,
        from_block: u64,
        to_block: u64,
    ) -> Result<(), String>;

    async fn get_last_processed_block(&mut self, src_chain_id: L2ChainId) -> Result<u64, String>;

    async fn inject_new_fee_bundle(
        &mut self,
        tx_hash: H256,
        fee_bundle: Vec<u8>,
    ) -> Result<(), String>;
}

pub struct InteropSwitch<C: DbClient> {
    src_chains: Vec<Chain>,
    dst_chains: Vec<Chain>,
    db: C,
}

pub struct InteropSender<C: DbClient> {
    dst_chain: Chain,
    _phantom_data: PhantomData<C>,
}

impl<C: DbClient> InteropSender<C> {
    pub async fn start(&self, db: &C) -> Result<(), String> {
        // Start sending interop transactions to the destination chain
        // and handle them accordingly.
        Ok(())
    }
}

impl<C: DbClient> InteropSwitch<C> {
    pub fn new(src_chains: Vec<Chain>, dst_chains: Vec<Chain>, db: C) -> Self {
        Self {
            src_chains,
            dst_chains,
            db,
        }
    }

    pub async fn start(&mut self) -> Result<(), String> {
        for src_chain in &self.src_chains {
            for dst_chain in &self.dst_chains {
                let listener = InteropListener::new(src_chain.clone(), dst_chain.chain_id, 100);
                listener.start(&mut self.db).await?;
            }
        }

        for dst_chain in &self.dst_chains {
            let sender = InteropSender {
                dst_chain: dst_chain.clone(),
                _phantom_data: Default::default(),
            };
            sender.start(&self.db).await?;
        }

        Ok(())
    }
}
