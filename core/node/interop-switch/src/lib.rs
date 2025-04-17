use std::marker::PhantomData;
use zksync_basic_types::{L2ChainId, H256};
use zksync_web3_decl::client::{Client, L2};

pub struct InteropTx {
    pub tx_hash: H256,
    pub src_chain_id: L2ChainId,
    pub dst_chain_id: L2ChainId,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Chain {
    pub chain_id: L2ChainId,
    pub client: Client<L2>,
}

#[async_trait::async_trait]
pub trait DbClient {
    async fn save_interop_tx(&mut self, tx: InteropTx) -> Result<(), String>;
    async fn get_interop_tx(&mut self, tx_hash: H256) -> Result<(), String>;
    async fn commit_interop_tx(&mut self, tx_hash: H256) -> Result<(), String>;
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

pub struct InteropListener<C: DbClient> {
    src_chain: Chain,
    dst_chain: L2ChainId,
    _phantom_data: PhantomData<C>,
}

impl<C: DbClient> InteropListener<C> {
    pub async fn start(&self, db: &C) -> Result<(), String> {
        // Start listening for interop transactions on the source chain
        // and handle them accordingly.
        Ok(())
    }
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
                let listener = InteropListener {
                    src_chain: src_chain.clone(),
                    dst_chain: dst_chain.chain_id,
                    _phantom_data: Default::default(),
                };
                listener.start(&self.db).await?;
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
