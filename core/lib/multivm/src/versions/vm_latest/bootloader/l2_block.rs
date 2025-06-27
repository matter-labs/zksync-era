use std::cmp::Ordering;

use zksync_types::{web3::keccak256_concat, InteropRoot, L2BlockNumber, H256};

use crate::{
    interface::{L2Block, L2BlockEnv},
    vm_latest::{
        bootloader::{snapshot::L2BlockSnapshot, tx::BootloaderTx},
        utils::l2_blocks::l2_block_hash,
    },
};

const EMPTY_TXS_ROLLING_HASH: H256 = H256::zero();

#[derive(Debug)]
pub(crate) struct BootloaderL2Block {
    pub(crate) number: u32,
    pub(crate) timestamp: u64,
    pub(crate) txs_rolling_hash: H256, // The rolling hash of all the transactions in the miniblock
    pub(crate) prev_block_hash: H256,
    // Number of the first L2 block tx in L1 batch
    pub(crate) first_tx_index: usize,
    pub(crate) max_virtual_blocks_to_create: u32,
    pub(crate) txs: Vec<BootloaderTx>,
    pub(crate) interop_roots: Vec<InteropRoot>,
}

impl BootloaderL2Block {
    pub(crate) fn new(l2_block: L2BlockEnv, first_tx_place: usize) -> Self {
        Self {
            number: l2_block.number,
            timestamp: l2_block.timestamp,
            txs_rolling_hash: EMPTY_TXS_ROLLING_HASH,
            prev_block_hash: l2_block.prev_block_hash,
            first_tx_index: first_tx_place,
            max_virtual_blocks_to_create: l2_block.max_virtual_blocks_to_create,
            txs: vec![],
            interop_roots: vec![],
        }
    }

    pub(super) fn push_tx(&mut self, tx: BootloaderTx) {
        self.update_rolling_hash(tx.hash);
        self.txs.push(tx)
    }

    pub(crate) fn get_hash(&self) -> H256 {
        l2_block_hash(
            L2BlockNumber(self.number),
            self.timestamp,
            self.prev_block_hash,
            self.txs_rolling_hash,
        )
    }

    fn update_rolling_hash(&mut self, tx_hash: H256) {
        self.txs_rolling_hash = keccak256_concat(self.txs_rolling_hash, tx_hash)
    }

    pub(crate) fn make_snapshot(&self) -> L2BlockSnapshot {
        L2BlockSnapshot {
            txs_rolling_hash: self.txs_rolling_hash,
            txs_len: self.txs.len(),
        }
    }

    pub(crate) fn apply_snapshot(&mut self, snapshot: L2BlockSnapshot) {
        self.txs_rolling_hash = snapshot.txs_rolling_hash;
        match self.txs.len().cmp(&snapshot.txs_len) {
            Ordering::Greater => self.txs.truncate(snapshot.txs_len),
            Ordering::Less => panic!("Applying snapshot from future is not supported"),
            Ordering::Equal => {}
        }
    }
    pub(crate) fn l2_block(&self) -> L2Block {
        L2Block {
            number: self.number,
            timestamp: self.timestamp,
            hash: self.get_hash(),
        }
    }
}
