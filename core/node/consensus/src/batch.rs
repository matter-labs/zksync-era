//! L1 Batch representation for sending over p2p network.
use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _};
use zksync_consensus_roles::validator;
use zksync_dal::consensus_dal::Payload;
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

/// Commitment to the last block of a batch.
pub(crate) struct LastBlockCommit {
    /// Hash of the `StoredBatchInfo` which is stored on L1.
    /// The hashed `StoredBatchInfo` contains a `root_hash` of the L2 state,
    /// which contains state of the `SystemContext` contract,
    /// which contains enough data to reconstruct the hash
    /// of the last L2 block of the batch.
    pub(crate) info: H256,
}

/// Witness proving what is the last block of a batch.
/// Contains the hash and the number of the last block.
pub(crate) struct LastBlockWitness {
    info: i_executor::structures::StoredBatchInfo,
    protocol_version: ProtocolVersionId,

    current_l2_block_info: TreeEntryWithProof,
    tx_rolling_hash: TreeEntryWithProof,
    l2_block_hash_entry: TreeEntryWithProof,
}

/// Commitment to an L1 batch.
pub(crate) struct L1BatchCommit {
    pub(crate) number: L1BatchNumber,
    pub(crate) this_batch: LastBlockCommit,
    pub(crate) prev_batch: LastBlockCommit,
}

/// L1Batch with witness that can be
/// verified against `L1BatchCommit`.
pub struct L1BatchWithWitness {
    pub(crate) blocks: Vec<Payload>,
    pub(crate) this_batch: LastBlockWitness,
    pub(crate) prev_batch: LastBlockWitness,
}

impl LastBlockWitness {
    /// Address of the SystemContext contract.
    fn system_context_addr() -> AccountTreeId {
        AccountTreeId::new(constants::SYSTEM_CONTEXT_ADDRESS)
    }

    /// Storage key of the `SystemContext.current_l2_block_info` field.
    fn current_l2_block_info_key() -> U256 {
        StorageKey::new(
            Self::system_context_addr(),
            constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
        )
        .hashed_key_u256()
    }

    /// Storage key of the `SystemContext.tx_rolling_hash` field.
    fn tx_rolling_hash_key() -> U256 {
        StorageKey::new(
            Self::system_context_addr(),
            constants::SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
        )
        .hashed_key_u256()
    }

    /// Storage key of the entry of the `SystemContext.l2BlockHash[]` array, corresponding to l2
    /// block with number i.
    fn l2_block_hash_entry_key(i: L2BlockNumber) -> U256 {
        let key = h256_to_u256(constants::SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION)
            + U256::from(i.0) % U256::from(constants::SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
        StorageKey::new(Self::system_context_addr(), u256_to_h256(key)).hashed_key_u256()
    }

    /// Loads a `LastBlockWitness` from storage.
    async fn load(
        ctx: &ctx::Ctx,
        n: L1BatchNumber,
        pool: &ConnectionPool,
        tree: &dyn TreeApiClient,
    ) -> ctx::Result<Self> {
        let mut conn = pool.connection(ctx).await.wrap("pool.connection()")?;
        let batch = conn
            .batch(ctx, n)
            .await
            .wrap("batch()")?
            .context("batch not in storage")?;

        let proofs = tree
            .get_proofs(
                n,
                vec![
                    Self::current_l2_block_info_key(),
                    Self::tx_rolling_hash_key(),
                ],
            )
            .await
            .context("get_proofs()")?;
        if proofs.len() != 2 {
            return Err(anyhow::format_err!("proofs.len()!=2").into());
        }
        let current_l2_block_info = proofs[0].clone();
        let tx_rolling_hash = proofs[1].clone();
        let (block_number, _) = unpack_block_info(current_l2_block_info.value.as_bytes().into());
        let prev = L2BlockNumber(
            block_number
                .checked_sub(1)
                .context("L2BlockNumber underflow")?
                .try_into()
                .context("L2BlockNumber overflow")?,
        );
        let proofs = tree
            .get_proofs(n, vec![Self::l2_block_hash_entry_key(prev)])
            .await
            .context("get_proofs()")?;
        if proofs.len() != 1 {
            return Err(anyhow::format_err!("proofs.len()!=1").into());
        }
        let l2_block_hash_entry = proofs[0].clone();
        Ok(Self {
            info: i_executor::structures::StoredBatchInfo::from(&batch),
            protocol_version: batch
                .header
                .protocol_version
                .context("missing protocol_version")?,

            current_l2_block_info,
            tx_rolling_hash,
            l2_block_hash_entry,
        })
    }

    /// Verifies the proof against the commit and returns the hash
    /// of the last L2 block.
    pub(crate) fn verify(&self, comm: &LastBlockCommit) -> anyhow::Result<(L2BlockNumber, H256)> {
        // Verify info.
        anyhow::ensure!(comm.info == self.info.hash());

        // Check the protocol version.
        anyhow::ensure!(
            self.protocol_version >= ProtocolVersionId::Version13,
            "unsupported protocol version"
        );

        let (block_number, block_timestamp) =
            unpack_block_info(self.current_l2_block_info.value.as_bytes().into());
        let prev = L2BlockNumber(
            block_number
                .checked_sub(1)
                .context("L2BlockNumber underflow")?
                .try_into()
                .context("L2BlockNumber overflow")?,
        );

        // Verify merkle paths.
        self.current_l2_block_info
            .verify(Self::current_l2_block_info_key(), self.info.batch_hash)
            .context("invalid merkle path for current_l2_block_info")?;
        self.tx_rolling_hash
            .verify(Self::tx_rolling_hash_key(), self.info.batch_hash)
            .context("invalid merkle path for tx_rolling_hash")?;
        self.l2_block_hash_entry
            .verify(Self::l2_block_hash_entry_key(prev), self.info.batch_hash)
            .context("invalid merkle path for l2_block_hash entry")?;

        let block_number = L2BlockNumber(block_number.try_into().context("block_number overflow")?);
        // Derive hash of the last block
        Ok((
            block_number,
            L2BlockHasher::hash(
                block_number,
                block_timestamp,
                self.l2_block_hash_entry.value,
                self.tx_rolling_hash.value,
                self.protocol_version,
            ),
        ))
    }

    /// Last L2 block of the batch.
    pub fn last_block(&self) -> validator::BlockNumber {
        let (n, _) = unpack_block_info(self.current_l2_block_info.value.as_bytes().into());
        validator::BlockNumber(n)
    }
}

impl L1BatchWithWitness {
    /// Loads an `L1BatchWithWitness` from storage.
    pub(crate) async fn load(
        ctx: &ctx::Ctx,
        number: L1BatchNumber,
        pool: &ConnectionPool,
        tree: &dyn TreeApiClient,
    ) -> ctx::Result<Self> {
        let prev_batch = LastBlockWitness::load(ctx, number - 1, pool, tree)
            .await
            .with_wrap(|| format!("LastBlockWitness::make({})", number - 1))?;
        let this_batch = LastBlockWitness::load(ctx, number, pool, tree)
            .await
            .with_wrap(|| format!("LastBlockWitness::make({number})"))?;
        let mut conn = pool.connection(ctx).await.wrap("connection()")?;
        let this = Self {
            blocks: conn
                .payloads(
                    ctx,
                    std::ops::Range {
                        start: prev_batch.last_block() + 1,
                        end: this_batch.last_block() + 1,
                    },
                )
                .await
                .wrap("payloads()")?,
            prev_batch,
            this_batch,
        };
        Ok(this)
    }

    /// Verifies the L1Batch and witness against the commitment.
    /// WARNING: the following fields of the payload are not currently verified:
    /// * `l1_gas_price`
    /// * `l2_fair_gas_price`
    /// * `fair_pubdata_price`
    /// * `virtual_blocks`
    /// * `operator_address`
    /// * `protocol_version` (present both in payload and witness, but neither has a commitment)
    pub(crate) fn verify(&self, comm: &L1BatchCommit) -> anyhow::Result<()> {
        let (last_number, last_hash) = self.this_batch.verify(&comm.this_batch)?;
        let (mut prev_number, mut prev_hash) = self.prev_batch.verify(&comm.prev_batch)?;
        anyhow::ensure!(
            self.prev_batch
                .info
                .batch_number
                .checked_add(1)
                .context("batch_number overflow")?
                == u64::from(comm.number.0)
        );
        anyhow::ensure!(self.this_batch.info.batch_number == u64::from(comm.number.0));
        for (i, b) in self.blocks.iter().enumerate() {
            anyhow::ensure!(b.l1_batch_number == comm.number);
            anyhow::ensure!(b.protocol_version == self.this_batch.protocol_version);
            anyhow::ensure!(b.last_in_batch == (i + 1 == self.blocks.len()));
            prev_number += 1;
            let mut hasher = L2BlockHasher::new(prev_number, b.timestamp, prev_hash);
            for t in &b.transactions {
                // Reconstruct transaction by converting it back and forth to `abi::Transaction`.
                // This allows us to verify that the transaction actually matches the transaction
                // hash.
                // TODO: make consensus payload contain `abi::Transaction` instead.
                // TODO: currently the payload doesn't contain the block number, which is
                // annoying. Consider adding it to payload.
                let t2: Transaction = abi::Transaction::try_from(t.clone())?.try_into()?;
                anyhow::ensure!(t == &t2);
                hasher.push_tx_hash(t.hash());
            }
            prev_hash = hasher.finalize(self.this_batch.protocol_version);
            anyhow::ensure!(prev_hash == b.hash);
        }
        anyhow::ensure!(prev_hash == last_hash);
        anyhow::ensure!(prev_number == last_number);
        Ok(())
    }
}
