//! L1 Batch representation for sending over p2p network.
use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _};
use zksync_consensus_roles::{attester,validator};
use zksync_dal::consensus_dal::{BlockMetadata, BatchProof, BlockDigest,StorageProof,Payload};
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

fn tree_entry(proof: &StorageProof) -> TreeEntryWithProof {
    TreeEntryWithProof {
        value: proof.value,
        index: proof.index,
        merkle_path: proof.merkle_path.clone(),
    }
}

fn storage_proof(e: TreeEntryWithProof) -> StorageProof {
    StorageProof {
        value: e.value,
        index: e.index,
        merkle_path: e.merkle_path,
    }
}

#[derive(Debug,PartialEq)]
pub(crate) struct VerifiedBatchProof(BatchProof);

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
            unpack_block_info(proof.current_l2_block_info.value.as_bytes().into());
        let last_block_number = u32::try_from(last_block_number).context("overflow")?;
        let prev_block_number = last_block_number.checked_sub(1).context("overflow")?;
        let want = L2BlockHasher {
            number: L2BlockNumber(prev_block_number),
            timestamp: last_block_timestamp,
            prev_l2_block_hash: proof.l2_block_hash_entry.value,
            txs_rolling_hash: proof.tx_rolling_hash.value,
        }.finalize(protocol_version);

        /// Compute the hash of the last block, implied by the digests.
        let block_count = u32::try_from(proof.blocks.len()).context("overflow")?;
        let first_block = last_block_number.checked_sub(block_count-1).context("overflow")?;
        let mut got = proof.initial_hash;
        for i in 0..block_count {
            let b = &proof.blocks[usize::try_from(i).unwrap()];
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
        tree_entry(&proof.current_l2_block_info)
            .verify(current_l2_block_info_key(), proof.info.batch_hash)
            .context("invalid merkle path for current_l2_block_info")?;
        tree_entry(&proof.tx_rolling_hash)
            .verify(tx_rolling_hash_key(), proof.info.batch_hash)
            .context("invalid merkle path for tx_rolling_hash")?;
        tree_entry(&proof.l2_block_hash_entry)
            .verify(l2_block_hash_entry_key(L2BlockNumber(prev_block_number)), proof.info.batch_hash)
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
        if !r.contains(&n) { return None }
        self.blocks.get((n.0-r.start.0) as usize)
    }

    pub fn verify_payload(&self, n: validator::BlockNumber, meta: &BlockMetadata, payload: &Payload) -> anyhow::Result<()> {
        anyhow::ensure!(meta.batch_number == self.batch_number(), "batch number mismatch");
        anyhow::ensure!(meta.protocol_version == payload.protocol_version, "protocol version mismatch");
        anyhow::ensure!(meta.l1_gas_price == payload.l1_gas_price, "l1 gas price mismatch");
        anyhow::ensure!(meta.l2_fair_gas_price == payload.l2_fair_gas_price, "l2 fair gas price mismatch");
        anyhow::ensure!(meta.fair_pubdata_price == payload.fair_pubdata_price, "fair pubdata price mismatch");
        anyhow::ensure!(meta.virtual_blocks == payload.virtual_blocks, "virtual blocks mismatch");
        anyhow::ensure!(meta.operator_address == payload.operator_address, "operator address mismatch");
        anyhow::ensure!(self.digest(n).context("digest()")? == &BlockDigest::from_payload(payload), "digest mismatch");
        Ok(())
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
        let (first_block,last_block) = conn.get_l2_block_range_of_l1_batch(ctx,n).await
            .wrap("get_l2_block_range_of_l1_batch")?.context("batch is missing")?;
        let payloads = conn.payloads(ctx, first_block..last_block+1)
            .await
            .wrap("payloads()")?;
        // The initial rolling block hash is just hash of the last block of the previous batch.
        let initial_hash = conn.payload(ctx, first_block.prev().context("overflow")?).await.wrap("payload()")?.context("missing block")?.hash;
        drop(conn);    
   
        /// Fetch storage proofs.
        let prev_block = L2BlockNumber(last_block.0.checked_sub(1).context("overflow")?.try_into().context("overflow")?);
        let [
            current_l2_block_info,
            tx_rolling_hash,
            l2_block_hash_entry,
        ] : [_; 3] = tree
            .get_proofs(
                L1BatchNumber(n.0.try_into().context("overflow")?),
                vec![
                    current_l2_block_info_key(),
                    tx_rolling_hash_key(),
                    l2_block_hash_entry_key(prev_block),
                ],
            )
            .await
            .context("get_proofs()")?
            .try_into()
            .ok().context("couldn't fetch storage proofs")?;
        
        Ok(BatchProof {
            info,
            current_l2_block_info: storage_proof(current_l2_block_info),
            tx_rolling_hash: storage_proof(tx_rolling_hash),
            l2_block_hash_entry: storage_proof(l2_block_hash_entry),
            initial_hash,
            blocks: payloads.iter().map(BlockDigest::from_payload).collect(),
        })
    }
}
