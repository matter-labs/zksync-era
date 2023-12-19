use zksync_consensus_storage::ReplicaState;

use crate::models::storage_sync::ConsensusBlockFields;
use crate::StorageProcessor;
use zksync_consensus_roles::validator;
use zksync_types::MiniblockNumber;

#[derive(Debug)]
pub struct ConsensusDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ConsensusDal<'_, '_> {
    pub async fn replica_state(&mut self) -> anyhow::Result<Option<ReplicaState>> {
        let Some(row) =
            sqlx::query!("SELECT state as \"state!\" FROM consensus_replica_state WHERE fake_key")
                .fetch_optional(self.storage.conn())
                .await?
        else {
            return Ok(None);
        };
        Ok(Some(zksync_protobuf::serde::deserialize(row.state)?))
    }

    pub async fn set_replica_state(&mut self, state: &ReplicaState) -> sqlx::Result<()> {
        let state =
            zksync_protobuf::serde::serialize(state, serde_json::value::Serializer).unwrap();
        sqlx::query!("INSERT INTO consensus_replica_state(fake_key,state) VALUES(true,$1) ON CONFLICT (fake_key) DO UPDATE SET state = excluded.state", state)
            .execute(self.storage.conn())
            .await?;
        Ok(())
    }

    /// Fetches the number of the last miniblock with consensus fields set.
    /// Miniblocks with Consensus fields set constitute a prefix of sealed miniblocks,
    /// so it is enough to traverse the miniblocks in descending order to find the last
    /// with consensus fields.
    ///
    /// If better efficiency is needed we can add an index on "miniblocks without consensus fields".
    pub async fn get_last_block_number(&mut self) -> anyhow::Result<Option<validator::BlockNumber>> {
        let Some(row) = sqlx::query!("SELECT number FROM miniblocks_consensus ORDER BY number DESC LIMIT 1")
            .fetch_optional(self.storage.conn())
            .await? else { return Ok(None) };
        Ok(Some(validator::BlockNumber(row.number.try_into()?)))
    }

    pub async fn get_first_block_number(&mut self) -> anyhow::Result<Option<validator::BlockNumber>> {
        let Some(row) = sqlx::query!("SELECT number FROM miniblocks_consensus ORDER BY number ASC LIMIT 1")
            .fetch_optional(self.storage.conn())
            .await? else { return Ok(None) };
        Ok(Some(validator::BlockNumber(row.number.try_into()?)))
    }

    /// Checks whether the specified miniblock has consensus field set.
    /*pub async fn has_consensus_fields(&mut self, number: MiniblockNumber) -> sqlx::Result<bool> {
        Ok(sqlx::query!("SELECT COUNT(*) as \"count!\"  FROM miniblocks WHERE number = $1 AND consensus IS NOT NULL", number.0 as i64)
            .fetch_one(self.storage.conn())
            .await?
            .count > 0)
    }*/

    pub async fn block_payload(&mut self, block_number: validator::BlockNumber, operator_address: Address) -> anyhow::Result<Option<Payload>> {
        let block_number = MiniblockNumber(block_number.0.try_into()?);
        let Some(block) = self.storage.sync_dal().sync_block_inner(block_number).await? else { return Ok(None) };
        let transactions = self.storage.sync_dal().sync_block_transactions(block_number).await?;
        Ok(block.into_payload(operator_address,transactions))
    }

    /// Sets consensus-related fields for the specified miniblock.
    pub async fn insert_quorum_ceritificate(&mut self, qc: &validator::CommitQC) -> anyhow::Result<()> {
        let txn = self.storage.start_transaction().await?;
        if let Some(head) = self.get_last_block_number().await? {
            anyhow::ensure!(head.next()==block.header.number,"expected a next block after the current head block");
            // TODO: compare parent hash.
        }
        let want_payload = self.block_payload(block.header.number).await?
            .context("corresponding miniblock is missing")?;
        anyhow::ensure!(got_payload==want_payload,"consensus block payload doesn't match the miniblock");
        let result = sqlx::query!(
            "INSERT INTO miniblocks_consensus (number, fields) VALUES ($1, $2)",
            block.header.number.0 as i64,
            zksync_protobuf::serde::serialize(&block.justification, serde_json::value::Serializer),
            .unwrap(),
        )
            .execute(self.storage.conn())
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
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        assert!(conn
            .consensus_dal()
            .replica_state()
            .await
            .unwrap()
            .is_none());
        let rng = &mut rand::thread_rng();
        for _ in 0..10 {
            let want: ReplicaState = rng.gen();
            conn.consensus_dal().set_replica_state(&want).await.unwrap();
            assert_eq!(
                Some(want),
                conn.consensus_dal().replica_state().await.unwrap()
            );
        }
    }
}
