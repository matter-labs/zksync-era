use anyhow::Context as _;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::ReplicaState;
use zksync_types::MiniblockNumber;

pub use crate::models::storage_sync::Payload;
use crate::StorageProcessor;

/// Storage access methods for `zksync_core::consensus` module.
#[derive(Debug)]
pub struct ConsensusDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ConsensusDal<'_, '_> {
    /// Fetches genesis.
    pub async fn genesis(&mut self) -> anyhow::Result<Option<validator::Genesis>> {
        let Some(row) = sqlx::query!("SELECT genesis FROM consensus_replica_state WHERE fake_key")
            .fetch_optional(self.storage.conn())
            .await?
        else { return Ok(None); };
        let Some(genesis) = row.genesis else { return Ok(None); };
        Ok(Some(zksync_protobuf::serde::deserialize(genesis)?))
    }

    /// Updates the genesis.
    pub async fn try_update_genesis(&mut self, genesis: &validator::Genesis) -> anyhow::Result<()> {
        if genesis.forks.iter().count()!=1 {
            anyhow::bail!("many-fork genesis unsupported");
        }
        let mut txn = self.storage.start_transaction().await?;
        if let Some(got) = txn.consensus_dal().genesis().await? {
            // Exit if the genesis didn't change.
            if &got==genesis { return Ok(()); }
            if got.forks.current().number >= genesis.forks.current().number {
                anyhow::bail!("transition to a past fork is not allowed");
            }
        }
        let genesis = zksync_protobuf::serde::serialize(genesis, serde_json::value::Serializer).unwrap();
        let state = zksync_protobuf::serde::serialize(&ReplicaState::default(), serde_json::value::Serializer).unwrap();
        sqlx::query!("DELETE FROM miniblocks_consensus").execute(txn.conn()).await?;
        sqlx::query!("DELETE FROM consensus_replica_state").execute(txn.conn()).await?;
        sqlx::query!("INSERT INTO consensus_replica_state (fake_key, genesis, state) VALUES(TRUE, $1, $2)",
            genesis,
            state,
        ).execute(txn.conn()).await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn fork(&mut self, first_block: validator::BlockNumber) -> anyhow::Result<()> {
        let mut txn = self.storage.start_transaction().await?;
        let Some(old) = txn.consensus_dal().genesis().await? else { return Ok(()); };
        let new = validator::Genesis {
            validators: old.validators,
            forks: validator::ForkSet::new(vec![validator::Fork {
                number: old.forks.current().number,
                first_block,
                first_parent: None,
            }]).unwrap(),
        };
        txn.consensus_dal().try_update_genesis(&new).await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn try_init_genesis(&mut self, validators: &validator::ValidatorSet) -> anyhow::Result<()> {
        let first_block = self.storage.blocks_dal().get_sealed_miniblock_number().await?.unwrap_or(0.into());
        let first_block = validator::BlockNumber(first_block.0.into());
        let mut txn = self.storage.start_transaction().await?;
        if txn.consensus_dal().genesis().await?.is_some() {
            return Ok(());
        }
        let genesis = validator::Genesis {
            validators: validators.clone(),
            forks: validator::ForkSet::new(vec![validator::Fork {
                number: validator::ForkNumber(0),
                first_block,
                first_parent: None,
            }]).unwrap(),
        };
        txn.consensus_dal().try_update_genesis(&genesis).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Fetches the current BFT replica state.
    pub async fn replica_state(&mut self) -> anyhow::Result<ReplicaState> {
        let row = sqlx::query!(
            r#"
            SELECT
                state as "state!"
            FROM
                consensus_replica_state
            WHERE
                fake_key
            "#
        )
        .fetch_one(self.storage.conn())
        .await?;
        Ok(zksync_protobuf::serde::deserialize(row.state)?)
    }

    /// Sets the current BFT replica state.
    pub async fn set_replica_state(&mut self, state: &ReplicaState) -> sqlx::Result<()> {
        let state = zksync_protobuf::serde::serialize(state, serde_json::value::Serializer).unwrap();
        sqlx::query!("UPDATE consensus_replica_state SET state = $1 WHERE fake_key", state)
            .execute(self.storage.conn())
            .await?;
        Ok(())
    }

    /// Fetches the first consensus certificate.
    /// Note that we didn't backfill the certificates for the past miniblocks
    /// when enabling consensus certificate generation, so it might NOT be the certificate
    /// for the genesis miniblock.
    pub async fn first_certificate(&mut self) -> anyhow::Result<Option<validator::CommitQC>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                certificate
            FROM
                miniblocks_consensus
            ORDER BY
                number ASC
            LIMIT
                1
            "#
        )
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(zksync_protobuf::serde::deserialize(row.certificate)?))
    }

    /// Fetches the last consensus certificate.
    /// Currently certificates are NOT generated synchronously with miniblocks,
    /// so it might NOT be the certificate for the last miniblock.
    pub async fn last_certificate(&mut self) -> anyhow::Result<Option<validator::CommitQC>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                certificate
            FROM
                miniblocks_consensus
            ORDER BY
                number DESC
            LIMIT
                1
            "#
        )
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(zksync_protobuf::serde::deserialize(row.certificate)?))
    }

    /// Fetches the consensus certificate for the miniblock with the given `block_number`.
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
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(zksync_protobuf::serde::deserialize(row.certificate)?))
    }

    /// Converts the miniblock `block_number` into consensus payload. `Payload` is an
    /// opaque format for the miniblock that consensus understands and generates a
    /// certificate for it.
    pub async fn block_payload(
        &mut self,
        block_number: validator::BlockNumber,
    ) -> anyhow::Result<Option<Payload>> {
        let block_number = MiniblockNumber(block_number.0.try_into()?);
        let Some(block) = self
            .storage
            .sync_dal()
            .sync_block_inner(block_number)
            .await?
        else {
            return Ok(None);
        };
        let transactions = self
            .storage
            .transactions_web3_dal()
            .get_raw_miniblock_transactions(block_number)
            .await?;
        Ok(Some(block.into_payload(transactions)))
    }

    /// Inserts a certificate for the miniblock `cert.header().number`.
    /// It verifies that
    /// * the certified payload matches the miniblock in storage
    /// * the `cert.header().parent` matches the parent miniblock.
    /// * the parent block already has a certificate.
    /// NOTE: This is an extra secure way of storing a certificate,
    /// which will help us to detect bugs in the consensus implementation
    /// while it is "fresh". If it turns out to take too long,
    /// we can remove the verification checks later.
    pub async fn insert_certificate(&mut self, cert: &validator::CommitQC) -> anyhow::Result<()> {
        let header = &cert.message.proposal;
        let mut txn = self.storage.start_transaction().await?;
        if let Some(last) = txn.consensus_dal().last_certificate().await? {
            let last = &last.message.proposal;
            anyhow::ensure!(
                last.number.next() == header.number,
                "expected certificate for a block after the current head block"
            );
            anyhow::ensure!(Some(last.hash()) == header.parent, "parent block mismatch");
        } else {
            anyhow::ensure!(header.parent.is_none(), "inserting first block with parent");
        }
        let want_payload = txn
            .consensus_dal()
            .block_payload(cert.message.proposal.number)
            .await?
            .context("corresponding miniblock is missing")?;
        anyhow::ensure!(
            header.payload == want_payload.encode().hash(),
            "consensus block payload doesn't match the miniblock"
        );
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
        .execute(txn.conn())
        .await?;
        txn.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng as _;
    use zksync_consensus_storage::ReplicaState;

    use crate::ConnectionPool;

    #[tokio::test]
    async fn replica_state_read_write() {
        let rng = &mut rand::thread_rng();
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        conn.consensus_dal().try_init_genesis(&rng.gen()).await.unwrap();
        assert_eq!(ReplicaState::default(), conn.consensus_dal().replica_state().await.unwrap());
        for _ in 0..10 {
            let want: ReplicaState = rng.gen();
            conn.consensus_dal().set_replica_state(&want).await.unwrap();
            assert_eq!(
                want,
                conn.consensus_dal().replica_state().await.unwrap()
            );
        }
    }
}
