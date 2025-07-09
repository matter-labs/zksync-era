use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::SLChainId;

use crate::Core;

pub struct EthWatcherDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, Copy, Clone, sqlx::Type)]
#[sqlx(type_name = "event_type")]
pub enum EventType {
    ProtocolUpgrades,
    PriorityTransactions,
    ChainBatchRoot,
    ServerNotification,
    ProofRequestAcknowledged,
    ProofRequestProven,
}

impl EthWatcherDal<'_, '_> {
    // Returns last set value of next_block_to_process for given event_type and chain_id.
    // If the value was missing, initializes it with provided next_block_to_process value
    pub async fn get_or_set_next_block_to_process(
        &mut self,
        event_type: EventType,
        chain_id: SLChainId,
        next_block_to_process: u64,
    ) -> DalResult<u64> {
        let result = sqlx::query!(
            r#"
            SELECT
                next_block_to_process
            FROM
                processed_events
            WHERE
                type = $1
                AND chain_id = $2
            "#,
            event_type as EventType,
            chain_id.0 as i64
        )
        .instrument("get_or_set_next_block_to_process")
        .with_arg("event_type", &event_type)
        .with_arg("chain_id", &chain_id)
        .fetch_optional(self.storage)
        .await?;

        if let Some(row) = result {
            Ok(row.next_block_to_process as u64)
        } else {
            sqlx::query!(
                r#"
                INSERT INTO
                processed_events (
                    type,
                    chain_id,
                    next_block_to_process
                )
                VALUES
                ($1, $2, $3)
                "#,
                event_type as EventType,
                chain_id.0 as i64,
                next_block_to_process as i64
            )
            .instrument("get_or_set_next_block_to_process - insert")
            .with_arg("event_type", &event_type)
            .with_arg("chain_id", &chain_id)
            .execute(self.storage)
            .await?;

            Ok(next_block_to_process)
        }
    }

    pub async fn update_next_block_to_process(
        &mut self,
        event_type: EventType,
        chain_id: SLChainId,
        next_block_to_process: u64,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE processed_events
            SET
                next_block_to_process = $3
            WHERE
                type = $1
                AND chain_id = $2
            "#,
            event_type as EventType,
            chain_id.0 as i64,
            next_block_to_process as i64
        )
        .instrument("update_next_block_to_process")
        .with_arg("event_type", &event_type)
        .with_arg("chain_id", &chain_id)
        .execute(self.storage)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionPool, Core, CoreDal};

    #[tokio::test]
    async fn test_get_or_set_next_block_to_process_with_different_event_types() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let mut dal = conn.eth_watcher_dal();

        // Test with ProtocolUpgrades
        let next_block = dal
            .get_or_set_next_block_to_process(EventType::ProtocolUpgrades, SLChainId(1), 100)
            .await
            .expect("Failed to get or set next block to process");
        assert_eq!(next_block, 100);

        // Test with PriorityTransactions
        let next_block = dal
            .get_or_set_next_block_to_process(EventType::PriorityTransactions, SLChainId(1), 200)
            .await
            .expect("Failed to get or set next block to process");
        assert_eq!(next_block, 200);

        // Test with PriorityTransactions
        let next_block = dal
            .get_or_set_next_block_to_process(EventType::PriorityTransactions, SLChainId(2), 300)
            .await
            .expect("Failed to get or set next block to process");
        assert_eq!(next_block, 300);

        // Verify that the initial block is not updated for ProtocolUpgrades
        let next_block = dal
            .get_or_set_next_block_to_process(EventType::ProtocolUpgrades, SLChainId(1), 150)
            .await
            .expect("Failed to get or set next block to process");
        assert_eq!(next_block, 100);

        // Verify that the initial block is not updated for PriorityTransactions
        let next_block = dal
            .get_or_set_next_block_to_process(EventType::PriorityTransactions, SLChainId(1), 250)
            .await
            .expect("Failed to get or set next block to process");
        assert_eq!(next_block, 200);

        // Verify that the initial block is not updated for PriorityTransactions
        let next_block = dal
            .get_or_set_next_block_to_process(EventType::PriorityTransactions, SLChainId(2), 350)
            .await
            .expect("Failed to get or set next block to process");
        assert_eq!(next_block, 300);
    }
}
