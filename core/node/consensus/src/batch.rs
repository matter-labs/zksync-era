#![allow(unused)]
use zksync_types::{StorageKey, AccountTreeId, block::L2BlockHasher, block::unpack_block_info, ProtocolVersionId, L2BlockNumber, ethabi, Address, web3,L1BatchNumber,H256,U256};
use zksync_l1_contract_interface::i_executor;
use zksync_system_constants as constants;
use zksync_dal::consensus_dal::Payload;
//use zksync_merkle_tree::{TreeEntryWithProof};
use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_metadata_calculator::api_server::{TreeEntryWithProof,TreeApiClient};
use anyhow::Context as _;
use crate::ConnectionPool;
use zksync_concurrency::{ctx,error::Wrap as _};

/// Commitment to the last block of a batch.
struct LastBlockCommit {
    /// Hash of `StoredBatchInfoCompact`.
    info: H256,
}

/// Proof of the last block of the batch.
/// Contains the hash of the last block.
struct LastBlockProof {
    info: i_executor::structures::StoredBatchInfoCompact,
    protocol_version: ProtocolVersionId,

    current_l2_block_info: TreeEntryWithProof,
    tx_rolling_hash: TreeEntryWithProof,
    l2_block_hash_entry: TreeEntryWithProof,
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
    fn l2_block_hash_entry_key(i: U256) -> U256 {
        let key = U256::from(constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION.as_bytes()) +
            i % U256::from(constants::SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
        StorageKey::new(Self::addr(), <[u8;32]>::from(key).into()).hashed_key_u256()
    }

    async fn make(ctx: &ctx::Ctx, n: L1BatchNumber, pool: &ConnectionPool, tree: &dyn TreeApiClient) -> ctx::Result<Self> {
        let mut conn = pool.connection(ctx).await.wrap("pool.connection()")?;
        let batch = conn.batch(ctx,n).await.wrap("batch()")?.context("batch not in storage")?;

        let proofs = tree.get_proofs(n,vec![Self::current_l2_block_info_key(),Self::tx_rolling_hash_key()]).await.context("get_proofs()")?;
        if proofs.len()!=2 {
            return Err(anyhow::format_err!("proofs.len()!=2").into());
        }
        let current_l2_block_info = proofs[0].clone();
        let tx_rolling_hash = proofs[1].clone();
        let (block_number,_) = unpack_block_info(current_l2_block_info.value.as_bytes().into());
        let prev = block_number.checked_sub(1).context("block_number underflow")?;
        let proofs = tree.get_proofs(n,vec![Self::l2_block_hash_entry_key(prev.into())]).await.context("get_proofs()")?;
        if proofs.len()!=1 {
            return Err(anyhow::format_err!("proofs.len()!=1").into());
        }
        let l2_block_hash_entry = proofs[0].clone();
        Ok(Self {
            info: i_executor::structures::StoredBatchInfo(&batch).into_compact(),
            protocol_version: batch.header.protocol_version.context("missing protocol_version")?,

            current_l2_block_info,
            tx_rolling_hash, 
            l2_block_hash_entry,
        })
    }

    /// Verifies the proof against the commit and returns the hash
    /// of the last L2 block.
    pub fn verify(&self, comm: &LastBlockCommit) -> anyhow::Result<(L2BlockNumber,H256)> { 
        // Verify info.
        anyhow::ensure!(comm.info==self.info.hash());
        
        // Check the protocol version.
        anyhow::ensure!(self.protocol_version >= ProtocolVersionId::Version13, "unsupported protocol version");

        let (block_number,block_timestamp) = unpack_block_info(self.current_l2_block_info.value.as_bytes().into());
        let prev = block_number.checked_sub(1).context("block_number underflow")?;
        
        // Verify merkle paths.
        self.current_l2_block_info.verify(
            Self::current_l2_block_info_key(),
            self.info.batch_hash,
        )?;
        self.tx_rolling_hash.verify(
            Self::tx_rolling_hash_key(),
            self.info.batch_hash,
        )?;
        self.l2_block_hash_entry.verify(
            Self::l2_block_hash_entry_key(prev.into()),
            self.info.batch_hash,
        )?;

        // Derive hash of the last block
        Ok((
            L2BlockNumber(block_number.try_into().context("block_number overflow")?),
            L2BlockHasher::hash(
                L2BlockNumber(prev.try_into().context("block_number overflow")?),
                block_timestamp,
                self.l2_block_hash_entry.value,
                self.tx_rolling_hash.value,
                self.protocol_version,
            ),
        ))
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
    */ 

    /// WARNING: the following are not currently verified: l1_gas_price, l2_fair_gas_price, fair_pubdata_price,
    /// virtual_blocks, operator_address 
    fn verify(&self, comm: &L1BatchCommit) -> anyhow::Result<()> {
        let (last_number,last_hash) = self.this_batch.verify(&comm.this_batch)?;
        let (mut prev_number,mut prev_hash) = self.prev_batch.verify(&comm.prev_batch)?;
        anyhow::ensure!(self.prev_batch.info.batch_number.checked_add(1).context("batch_number overflow")?==u64::from(comm.number.0));
        anyhow::ensure!(self.this_batch.info.batch_number==u64::from(comm.number.0));
        for (i,b) in self.blocks.iter().enumerate() {
            anyhow::ensure!(b.l1_batch_number==comm.number);
            anyhow::ensure!(b.protocol_version==self.this_batch.protocol_version);
            anyhow::ensure!(b.last_in_batch == (i+1==self.blocks.len()));
            prev_number += 1;
            let mut hasher = L2BlockHasher::new(prev_number,b.timestamp,prev_hash);
            for t in &b.transactions {
                hasher.push_tx_hash(t.hash());
            }
            prev_hash = hasher.finalize(self.this_batch.protocol_version);
        }
        anyhow::ensure!(prev_hash==last_hash); 
        Ok(())
    }
}
