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

    pub async fn put_replica_state(&mut self, state: &ReplicaState) -> sqlx::Result<()> {
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
    pub async fn get_last_miniblock_with_consensus_fields(
        &mut self,
    ) -> anyhow::Result<Option<MiniblockNumber>> {
        let Some(row) = sqlx::query!("SELECT number FROM miniblocks WHERE consensus IS NOT NULL ORDER BY number DESC LIMIT 1")
            .fetch_optional(self.storage.conn())
            .await? else { return Ok(None) };
        Ok(Some(MiniblockNumber(row.number.try_into()?)))
    }

    /// Checks whether the specified miniblock has consensus field set.
    pub async fn has_consensus_fields(&mut self, number: MiniblockNumber) -> sqlx::Result<bool> {
        Ok(sqlx::query!("SELECT COUNT(*) as \"count!\"  FROM miniblocks WHERE number = $1 AND consensus IS NOT NULL", number.0 as i64)
            .fetch_one(self.storage.conn())
            .await?
            .count > 0)
    }

    /// Sets consensus-related fields for the specified miniblock.
    pub async fn set_consensus_fields(
        &mut self,
        block: &validator::FinalBlock,
    ) -> anyhow::Result<()> {
        let result = sqlx::query!(
            "UPDATE miniblocks SET consensus = $2 WHERE number = $1",
            block.header.number.0 as i64,
            zksync_protobuf::serde::serialize(
                &ConsensusBlockFields {
                    parent: block.header.parent,
                    justification: block.justification.clone(),
                },
                serde_json::value::Serializer
            )
            .unwrap(),
        )
        .execute(self.storage.conn())
        .await?;
        anyhow::ensure!(
            result.rows_affected() == 1,
            "Miniblock #{} is not present in Postgres",
            block.header.number.0,
        );
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
            conn.consensus_dal().put_replica_state(&want).await.unwrap();
            assert_eq!(
                Some(want),
                conn.consensus_dal().replica_state().await.unwrap()
            );
        }
    }
}
