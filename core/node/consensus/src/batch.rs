//! L1 Batch representation for sending over p2p network.
use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _};
use zksync_consensus_roles::{attester,validator};
use zksync_dal::consensus_dal::{BatchProof,BlockDigest,Payload};
use zksync_l1_contract_interface::i_executor;
use zksync_metadata_calculator::api_server::{TreeApiClient, TreeEntryWithProof};
use zksync_system_constants as constants;
use zksync_types::{
    abi,
    block::{unpack_block_info, L2BlockHasher},
    AccountTreeId, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey, Transaction, H256,
    U256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

use crate::storage::ConnectionPool;

/// Address of the SystemContext contract.
fn system_context_addr() -> AccountTreeId {
    AccountTreeId::new(constants::SYSTEM_CONTEXT_ADDRESS)
}

/// Storage key of the `SystemContext.current_l2_block_info` field.
fn current_l2_block_info_key() -> U256 {
    StorageKey::new(
        system_context_addr(),
        constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    )
    .hashed_key_u256()
}

/// Storage key of the `SystemContext.tx_rolling_hash` field.
fn tx_rolling_hash_key() -> U256 {
    StorageKey::new(
        system_context_addr(),
        constants::SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
    )
    .hashed_key_u256()
}

/// Storage key of the entry of the `SystemContext.l2BlockHash[]` array, corresponding to l2
/// block with number i.
fn l2_block_hash_entry_key(i: L2BlockNumber) -> U256 {
    let key = h256_to_u256(constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION)
        + U256::from(i.0) % U256::from(constants::SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
    StorageKey::new(system_context_addr(), u256_to_h256(key)).hashed_key_u256()
}

#[derive(Debug,PartialEq)]
struct VerifiedBatchProof(BatchProof);

impl std::ops::Deref for VerifiedBatchProof {
    type Target = BatchProof;
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl VerifiedBatchProof {
    pub fn new(proof: BatchProof, protocol_version: ProtocolVersionId) -> anyhow::Result<Self> {
        anyhow::ensure!(protocol_version >= ProtocolVersionId::Version13, "protocol version not supported");
        anyhow::ensure!(!proof.blocks.is_empty(), "empty batches not supported");
        
        /// Compute the hash of the last block, as implied by the storage proof.
        let (last_block_number, last_block_timestamp) =
            unpack_block_info(self.current_l2_block_info.value.as_bytes().into());
        let last_block_number = u32::try_from(last_block_number).context("overflow")?;
        let prev_block_number = last_block_number.checked_sub(1).context("overflow")? 
        let want = L2BlockHasher {
            number: L2BlockNumber(prev),
            timestamp: last_block_timestamp,
            prev_l2_block_hash: self.l2_block_hash_entry.value,
            txs_rolling_hash: self.tx_rolling_hash.value,
        }.finalize(protocol_version);

        /// Compute the hash of the last block, implied by the digests.
        let block_count = u32::try_from(proof.blocks.len()).context("overflow")?;
        let first_block = last_block_number.checked_sub(block_count-1).context("overflow")?;
        let mut got = proof.initial_hash;
        for i in 0..block_count {
            let b = &proof.blocks[i.into()];
            got = L2BlockHasher {
                number: L2BlockNumber(first_block+i),
                timestamp: b.timestamp,
                prev_l2_block_hash: got,
                txs_rolling_hash: b.txs_rolling_hash,
            }.finalize(protocol_version);
        }

        /// Check that they match.
        anyhow::ensure!(got == want, "digests do not match proof");

        // Verify merkle paths.
        proof.current_l2_block_info
            .verify(current_l2_block_info_key(), proof.info.batch_hash)
            .context("invalid merkle path for current_l2_block_info")?;
        proof.tx_rolling_hash
            .verify(tx_rolling_hash_key(), proof.info.batch_hash)
            .context("invalid merkle path for tx_rolling_hash")?;
        proof.l2_block_hash_entry
            .verify(l2_block_hash_entry_key(prev_block_number), proof.info.batch_hash)
            .context("invalid merkle path for l2_block_hash entry")?;

        Ok(Self(proof))
    }

    /// Range of blocks of the batch.
    pub fn block_range(&self) -> std::ops::Range<validator::BlockNumber> {
        let end = unpack_block_info(h256_to_u256(self.current_l2_block_info.value)).0;
        let start = end - (self.blocks.len()-1) as u64;
        validator::BlockNumber(start)..validator::BlockNumber(end)
    }

    /// Block digest corresponding the block number.
    pub fn digest(&self, n: validator::BlockNumber) -> Option<&BlockDigest> {
        let r = self.block_range();
        if !r.contains(n) { return None }
        self.blocks.get((n-r.start.0) as usize)
    }

    /// Loads a `LastBlockProof` from storage.
    pub async fn load(
        ctx: &ctx::Ctx,
        n: attester::BatchNumber,
        pool: &ConnectionPool,
        tree: &dyn TreeApiClient,
    ) -> ctx::Result<BatchProof> {
        let mut conn = pool.connection(ctx).await.wrap("pool.connection()")?;
        let info = conn
            .batch_info(ctx, n)
            .await
            .wrap("batch_info()")?
            .context("batch not in storage")?;
        let (first_block,last_block) = conn.get_l2_block_range_of_l1_batch(n).await
            .context("get_l2_block_range_of_l1_batch")?.context("batch is missing")?;
        let payloads = conn.payloads(ctx, first_block..last_block+1)
            .await
            .wrap("payloads()")?;
        drop(conn);    
   
        /// Fetch storage proofs.
        let prev_block = L2BlockNumber(last_block.0.checked_sub(1).context("overflow")?.try_into().context("overflow")?);
        let [
            current_l2_block_info,
            tx_rolling_hash,
            l2_block_hash_entry,
        ] : [TreeEntryWithProof; 3] = tree
            .get_proofs(
                n,
                vec![
                    current_l2_block_info_key(),
                    tx_rolling_hash_key(),
                    l2_block_hash_entry_key(prev_block),
                ],
            )
            .await
            .context("get_proofs()")?;
            .try_into()
            .context("couldn't fetch storage proofs")?;
        
        Ok(Self::new(BatchProof {
            info,
            current_l2_block_info,
            tx_rolling_hash,
            l2_block_hash_entry,
            blocks: payloads.iter().map(BlockDigest::from_payload).collect(),
        })?)
    }
}
