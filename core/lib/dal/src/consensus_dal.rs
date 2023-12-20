use zksync_consensus_storage::ReplicaState;

use crate::StorageProcessor;

#[derive(Debug)]
pub struct ConsensusDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ConsensusDal<'_, '_> {
    pub async fn replica_state(&mut self) -> anyhow::Result<Option<ReplicaState>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                state AS "state!"
            FROM
                consensus_replica_state
            WHERE
                fake_key
            "#
        )
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
        sqlx::query!(
            r#"
            INSERT INTO
                consensus_replica_state (fake_key, state)
            VALUES
                (TRUE, $1)
            ON CONFLICT (fake_key) DO
            UPDATE
            SET
                state = excluded.state
            "#,
            state
        )
        .execute(self.storage.conn())
        .await?;
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
