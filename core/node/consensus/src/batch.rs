use zksync_types::{StorageKey, AccountTreeId, block::unpack_block_info, api::StorageProof, ProtocolVersionId, L2BlockNumber, block::L2BlockHasher, ethabi, Address, web3,L1BatchNumber,H256,U256};
use zksync_l1_contract_interface::i_executor;
use zksync_system_constants as constants;
use zksync_dal::consensus_dal::Payload;
use zksync_merkle_tree::{TreeEntryWithProof};
use zksync_crypto::hasher::blake2::Blake2Hasher;
use anyhow::Context as _;

/// Commitment to the last block of a batch.
struct LastBlockCommit {
    /// Hash of `StoredBatchInfoCompact`.
    info: H256,
}

/// Proof of the last block of the batch.
/// Contains the hash of the last block.
struct LastBlockProof {
    info: i_executor::structures::StoredBatchInfoCompact,

    current_l2_block_info: TreeEntryWithProof,
    tx_rolling_hash: TreeEntryWithProof,
    l2_block_hashes_entry: TreeEntryWithProof,
}

/// Commitment to an L1 batch.
struct L1BatchCommit {
    number: L1BatchNumber,
    this_batch: LastBlockCommit,
    prev_batch: LastBlockCommit,
}

/// Proof of an L1 batch.
/// Contains the blocks of this batch.
struct L1BatchProof {
    blocks: Vec<Payload>,
    this_batch: LastBlockProof,
    prev_batch: LastBlockProof,
}

impl LastBlockProof {
    fn addr() -> AccountTreeId { AccountTreeId::new(constants::SYSTEM_CONTEXT_ADDRESS) }
    fn current_l2_block_info_key() -> U256 {
        StorageKey::new(Self::addr(), constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION).hashed_key_u256()
    }
    fn tx_rolling_hash_key() -> U256 {
        StorageKey::new(Self::addr(), constants::SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION).hashed_key_u256()
    }
    fn block_hashes_entry_key(i: U256) -> U256 {
        let key = U256::from(constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION.as_bytes()) +
            i % U256::from(constants::SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
        StorageKey::new(Self::addr(), <[u8;32]>::from(key).into()).hashed_key_u256()
    }


    /*async fn make(n: L1BatchNumber, pool: &ConnectionPool<Core>) -> anyhow::Result<Self> {
        let mut conn = pool.connection().await.context("pool.connection()")?;
        let batch = conn.blocks_dal().get_l1_batch_metadata(n).await?.context("batch not in storage")?

        let addr = constants::SYSTEM_CONTEXT_ADDRESS;
        let current_block_pos = constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION;
        let tx_rolling_hash_pos = constants::SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION;
        let resp = self.l2.get_proof(addr,vec![current_block_pos,tx_rolling_hash_pos],n).await.context("get_proof()")?.context("missing proof")?;
        let (block_number,_) = unpack_block_info(resp.storage_proof[0].value.as_bytes().into());
        let mut proof = resp.storage_proof;
        let prev_hash_pos =
            U256::from(constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION.as_bytes()) +
            U256::from(block_number-1) % U256::from(constants::SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
        let resp = self.l2.get_proof(addr,vec![<[u8;32]>::from(prev_hash_pos).into()],n).await.context("get_proof()")?.context("missing proof")?;
        Ok(Self {
            info: StoredBatchInfo(&batch).into_compact(),

            current_l2_block_info: StorageProof,
            tx_rolling_hash: StorageProof,
            l2_block_hashes_entry: StorageProof,
        })
    }
    */
    /// Verifies the proof against the commit and returns the hash
    /// of the last L2 block.
    pub fn verify(&self, comm: &LastBlockCommit) -> anyhow::Result<H256> {
        // Verify info.
        anyhow::ensure!(comm.0==self.info.hash());

        // Verify merkle paths.
        for p in [
            &self.current_l2_block_info,
            &self.tx_rolling_hash,
            &self.l2_block_hashes_entry,
        ] {
            p.verify(&Blake2Hasher,self.info.batch_hash)?;
        }

        // Verify entry keys.
        anyhow::ensure!(Self::current_l2_block_info_key()==self.current_l2_block_info.base.key);
        anyhow::ensure!(Self::tx_rolling_hash_key()==self.tx_rolling_hash.base.key);
        let (block_number,block_timestamp) = unpack_block_info(self.current_l2_block_info.base.value);
        let prev = block_number.checked_sub(1).context("block_number underflow")?;
        anyhow::ensure!(Self::l2_block_hashes_entry_key(prev.into())==self.l2_block_hashes_entry.base.key);

        // Derive hash of the last block
        Ok(Hasher(
            L2BlockNumber(block_number),
            self.l2_block_hashes_entry.base.value,
            block_timestamp,
            self.tx_rolling_hash.base.value,
        ).hash())
        
        // version in zksync_merkle_tree means l1 batch number
        // zksync_merkle_tree::types::TreeEntryWithProof::verify() 
        // it panics, so we need an error-returning version
        // Key is U256 (hashed key)
        // proof is a merkle path of length 256 with empty subtrees at the end skipped
    }
}

impl L1BatchProof {
    /*async fn make(&self, number: L1BatchNumber, pool: &ConnectionPool<Core>) -> anyhow::Result<Self> {
        Ok(Self {
            blocks: pool.get_batch_blocks(),
            this_batch: LastBlockProof::make(number).await?,
            prev_batch: LastBlockProof::make(number-1).await?,
        })
    }
    
    fn verify(&self, comm: &L1BatchCommit) -> anyhow::Result<()> {
        let last_hash = self.this_batch.verify(&comm.this_batch)?;
        let mut prev_hash = self.prev_batch.verify(&comm.prev_batch)?;
        anyhow::ensure!(self.prev_batch.info.number+1==comm.number);
        anyhow::ensure!(self.prev_batch.info.number==comm.number);
        let protocol_version = self.blocks[0].protocol_version;
        // TODO: check that protocol_version is bigger than last-supported-protocol_version
        for (i,b) in self.blocks.iter().enumerate() {
            anyhow::ensure!(b.batch_number==comm.number);
            anyhow::ensure!(b.protocol_version==protocol_version);
            anyhow::ensure!(b.last_in_batch == (i+1==self.blocks.len()));
            // compute verify the hash and timestamp of each block using the self.before and self.after as border
            //  conditions,
        }
        // TODO:
        // are these only for sealing conditions: l1_gas_price, l2_fair_gas_price, fair_pubdata_price,
        // do these affect execution: virtual_blocks, operator_address 
    }*/
}
