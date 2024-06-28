use anyhow::Context as _;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::{BlockStoreState, ReplicaState};
use zksync_db_connection::{
    connection::Connection,
    error::{DalError, DalResult, SqlxContext},
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::L2BlockNumber;

pub use crate::consensus::Payload;
use crate::{Core, CoreDal};

/// Storage access methods for `zksync_core::consensus` module.
#[derive(Debug)]
pub struct ConsensusDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

/// Error returned by `ConsensusDal::insert_certificate()`.
#[derive(thiserror::Error, Debug)]
pub enum InsertCertificateError {
    #[error("corresponding L2 block is missing")]
    MissingPayload,
    #[error("certificate doesn't match the payload")]
    PayloadMismatch,
    #[error(transparent)]
    Dal(#[from] DalError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl ConsensusDal<'_, '_> {
    /// Fetches genesis.
    pub async fn genesis(&mut self) -> DalResult<Option<validator::Genesis>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                genesis
            FROM
                consensus_replica_state
            WHERE
                fake_key
            "#
        )
        .try_map(|row| {
            let Some(genesis) = row.genesis else {
                return Ok(None);
            };
            let genesis: validator::GenesisRaw =
                zksync_protobuf::serde::deserialize(genesis).decode_column("genesis")?;
            Ok(Some(genesis.with_hash()))
        })
        .instrument("genesis")
        .fetch_optional(self.storage)
        .await?
        .flatten())
    }

    /// Attempts to update the genesis.
    /// Fails if the new genesis is invalid.
    /// Fails if the new genesis has different `chain_id`.
    /// Fails if the storage contains a newer genesis (higher fork number).
    /// Noop if the new genesis is the same as the current one.
    /// Resets the stored consensus state otherwise and purges all certificates.
    pub async fn try_update_genesis(&mut self, genesis: &validator::Genesis) -> anyhow::Result<()> {
        let mut txn = self.storage.start_transaction().await?;
        if let Some(got) = txn.consensus_dal().genesis().await? {
            // Exit if the genesis didn't change.
            if &got == genesis {
                return Ok(());
            }
            anyhow::ensure!(
                got.chain_id == genesis.chain_id,
                "changing chain_id is not allowed: old = {:?}, new = {:?}",
                got.chain_id,
                genesis.chain_id,
            );
            anyhow::ensure!(
                got.fork_number < genesis.fork_number,
                "transition to a past fork is not allowed: old = {:?}, new = {:?}",
                got.fork_number,
                genesis.fork_number,
            );
            genesis.verify().context("genesis.verify()")?;
        }
        let genesis =
            zksync_protobuf::serde::serialize(genesis, serde_json::value::Serializer).unwrap();
        let state = zksync_protobuf::serde::serialize(
            &ReplicaState::default(),
            serde_json::value::Serializer,
        )
        .unwrap();
        sqlx::query!(
            r#"
            DELETE FROM miniblocks_consensus
            "#
        )
        .instrument("try_update_genesis#DELETE FROM miniblock_consensus")
        .execute(&mut txn)
        .await?;
        sqlx::query!(
            r#"
            DELETE FROM consensus_replica_state
            "#
        )
        .instrument("try_update_genesis#DELETE FROM consensus_replica_state")
        .execute(&mut txn)
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO
                consensus_replica_state (fake_key, genesis, state)
            VALUES
                (TRUE, $1, $2)
            "#,
            genesis,
            state,
        )
        .instrument("try_update_genesis#INSERT INTO consenuss_replica_state")
        .execute(&mut txn)
        .await?;
        txn.commit().await?;
        Ok(())
    }

    /// [Main node only] creates a new consensus fork starting at
    /// the last sealed L2 block. Resets the state of the consensus
    /// by calling `try_update_genesis()`.
    pub async fn fork(&mut self) -> anyhow::Result<()> {
        let mut txn = self
            .storage
            .start_transaction()
            .await
            .context("start_transaction")?;
        let Some(old) = txn.consensus_dal().genesis().await.context("genesis()")? else {
            return Ok(());
        };
        let new = validator::GenesisRaw {
            chain_id: old.chain_id,
            fork_number: old.fork_number.next(),
            first_block: txn
                .consensus_dal()
                .next_block()
                .await
                .context("next_block()")?,

            protocol_version: old.protocol_version,
            validators: old.validators.clone(),
            attesters: old.attesters.clone(),
            leader_selection: old.leader_selection.clone(),
        }
        .with_hash();
        txn.consensus_dal().try_update_genesis(&new).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Fetches the current BFT replica state.
    pub async fn replica_state(&mut self) -> DalResult<ReplicaState> {
        sqlx::query!(
            r#"
            SELECT
                state AS "state!"
            FROM
                consensus_replica_state
            WHERE
                fake_key
            "#
        )
        .try_map(|row| zksync_protobuf::serde::deserialize(row.state).decode_column("state"))
        .instrument("replica_state")
        .fetch_one(self.storage)
        .await
    }

    /// Sets the current BFT replica state.
    pub async fn set_replica_state(&mut self, state: &ReplicaState) -> DalResult<()> {
        let state_json =
            zksync_protobuf::serde::serialize(state, serde_json::value::Serializer).unwrap();
        sqlx::query!(
            r#"
            UPDATE consensus_replica_state
            SET
                state = $1
            WHERE
                fake_key
            "#,
            state_json
        )
        .instrument("set_replica_state")
        .report_latency()
        .with_arg("state.view", &state.view)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// First block that should be in storage.
    async fn first_block(&mut self) -> anyhow::Result<validator::BlockNumber> {
        let info = self
            .storage
            .pruning_dal()
            .get_pruning_info()
            .await
            .context("get_pruning_info()")?;
        Ok(match info.last_soft_pruned_l2_block {
            // It is guaranteed that pruning info values are set for storage recovered from
            // snapshot, even if pruning was not enabled.
            Some(last_pruned) => validator::BlockNumber(last_pruned.0.into()) + 1,
            // No snapshot and no pruning:
            None => validator::BlockNumber(0),
        })
    }

    /// Next block that should be inserted to storage.
    pub async fn next_block(&mut self) -> anyhow::Result<validator::BlockNumber> {
        if let Some(last) = self
            .storage
            .blocks_dal()
            .get_sealed_l2_block_number()
            .await
            .context("get_sealed_l2_block_number()")?
        {
            return Ok(validator::BlockNumber(last.0.into()) + 1);
        }
        let next = self
            .storage
            .consensus_dal()
            .first_block()
            .await
            .context("first_block()")?;
        Ok(next)
    }

    /// Fetches the last consensus certificate.
    /// Currently, certificates are NOT generated synchronously with L2 blocks,
    /// so it might NOT be the certificate for the last L2 block.
    pub async fn certificates_range(&mut self) -> anyhow::Result<BlockStoreState> {
        // It cannot be older than genesis first block.
        let mut start = self.genesis().await?.context("genesis()")?.first_block;
        start = start.max(self.first_block().await.context("first_block()")?);
        let row = sqlx::query!(
            r#"
            SELECT
                certificate
            FROM
                miniblocks_consensus
            WHERE
                number >= $1
            ORDER BY
                number DESC
            LIMIT
                1
            "#,
            i64::try_from(start.0)?,
        )
        .instrument("last_certificate")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;
        Ok(BlockStoreState {
            first: start,
            last: row
                .map(|row| zksync_protobuf::serde::deserialize(row.certificate))
                .transpose()?,
        })
    }

    /// Fetches the consensus certificate for the L2 block with the given `block_number`.
    pub async fn certificate(
        &mut self,
        block_number: validator::BlockNumber,
    ) -> anyhow::Result<Option<validator::CommitQC>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                certificate
            FROM
                miniblocks_consensus
            WHERE
                number = $1
            "#,
            i64::try_from(block_number.0)?
        )
        .instrument("certificate")
        .report_latency()
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(zksync_protobuf::serde::deserialize(row.certificate)?))
    }

    /// Fetches a range of L2 blocks from storage and converts them to `Payload`s.
    pub async fn block_payloads(
        &mut self,
        numbers: std::ops::Range<validator::BlockNumber>,
    ) -> DalResult<Vec<Payload>> {
        let numbers = (|| {
            anyhow::Ok(std::ops::Range {
                start: L2BlockNumber(numbers.start.0.try_into().context("start")?),
                end: L2BlockNumber(numbers.end.0.try_into().context("end")?),
            })
        })()
        .map_err(|err| {
            Instrumented::new("block_payloads")
                .with_arg("numbers", &numbers)
                .arg_error("numbers", err)
        })?;

        let blocks = self
            .storage
            .sync_dal()
            .sync_blocks_inner(numbers.clone())
            .await?;
        let mut transactions = self
            .storage
            .transactions_web3_dal()
            .get_raw_l2_blocks_transactions(numbers)
            .await?;
        Ok(blocks
            .into_iter()
            .map(|b| {
                let txs = transactions.remove(&b.number).unwrap_or_default();
                b.into_payload(txs)
            })
            .collect())
    }

    /// Fetches an L2 block from storage and converts it to `Payload`. `Payload` is an
    /// opaque format for the L2 block that consensus understands and generates a
    /// certificate for it.
    pub async fn block_payload(
        &mut self,
        number: validator::BlockNumber,
    ) -> DalResult<Option<Payload>> {
        Ok(self
            .block_payloads(number..number + 1)
            .await?
            .into_iter()
            .next())
    }

    /// Inserts a certificate for the L2 block `cert.header().number`.
    /// Fails if certificate doesn't match the stored block.
    pub async fn insert_certificate(
        &mut self,
        cert: &validator::CommitQC,
    ) -> Result<(), InsertCertificateError> {
        use InsertCertificateError as E;
        let header = &cert.message.proposal;
        let mut txn = self.storage.start_transaction().await?;
        let want_payload = txn
            .consensus_dal()
            .block_payload(cert.message.proposal.number)
            .await?
            .ok_or(E::MissingPayload)?;
        if header.payload != want_payload.encode().hash() {
            return Err(E::PayloadMismatch);
        }
        sqlx::query!(
            r#"
            INSERT INTO
                miniblocks_consensus (number, certificate)
            VALUES
                ($1, $2)
            "#,
            header.number.0 as i64,
            zksync_protobuf::serde::serialize(cert, serde_json::value::Serializer).unwrap(),
        )
        .instrument("insert_certificate")
        .report_latency()
        .execute(&mut txn)
        .await?;
        txn.commit().await.context("commit")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng as _;
    use zksync_consensus_roles::validator;
    use zksync_consensus_storage::ReplicaState;

    use crate::{ConnectionPool, Core, CoreDal};

    #[tokio::test]
    async fn replica_state_read_write() {
        let rng = &mut rand::thread_rng();
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        assert_eq!(None, conn.consensus_dal().genesis().await.unwrap());
        for n in 0..3 {
            let setup = validator::testonly::Setup::new(rng, 3);
            let mut genesis = (*setup.genesis).clone();
            genesis.fork_number = validator::ForkNumber(n);
            let genesis = genesis.with_hash();
            conn.consensus_dal()
                .try_update_genesis(&genesis)
                .await
                .unwrap();
            assert_eq!(
                genesis,
                conn.consensus_dal().genesis().await.unwrap().unwrap()
            );
            assert_eq!(
                ReplicaState::default(),
                conn.consensus_dal().replica_state().await.unwrap()
            );
            for _ in 0..5 {
                let want: ReplicaState = rng.gen();
                conn.consensus_dal().set_replica_state(&want).await.unwrap();
                assert_eq!(
                    genesis,
                    conn.consensus_dal().genesis().await.unwrap().unwrap()
                );
                assert_eq!(want, conn.consensus_dal().replica_state().await.unwrap());
            }
        }
    }
}
