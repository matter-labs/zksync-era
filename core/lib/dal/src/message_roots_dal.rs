use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{
    h256_to_u256, message_root::MessageRoot, L1BatchNumber, L2BlockNumber, SLChainId, H256,
};

use crate::Core;

#[derive(Debug)]
pub struct MessageRootDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl MessageRootDal<'_, '_> {
    pub async fn save_message_root(&mut self, _msg_root: MessageRoot) -> DalResult<()> {
        Ok(())
    }

    pub async fn set_message_root(
        &mut self,
        chain_id: SLChainId,
        number: L1BatchNumber,
        message_root: &[H256],
        // proof: BatchAndChainMerklePath,
    ) -> DalResult<()> {
        println!(
            "set_message_root {:?} {:?} {:?}",
            chain_id.0, number.0, message_root
        );
        let sides = message_root
            .iter()
            .map(|root| root.as_bytes().to_vec())
            .collect::<Vec<_>>();
        sqlx::query!(
            r#"
            INSERT INTO MESSAGE_ROOTS (
                CHAIN_ID, DEPENDENCY_BLOCK_NUMBER, MESSAGE_ROOT_SIDES
            )
            VALUES ($1, $2, $3)
            ON CONFLICT (CHAIN_ID, DEPENDENCY_BLOCK_NUMBER)
            DO UPDATE SET MESSAGE_ROOT_SIDES = EXCLUDED.MESSAGE_ROOT_SIDES;
            "#,
            chain_id.0 as i64,
            i64::from(number.0),
            &sides
        )
        .instrument("set_message_root")
        .with_arg("chain_id", &chain_id)
        .with_arg("number", &number)
        .with_arg("message_root", &message_root)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_assgigned_roots_or_assign_if_needed(
        &mut self,
        processed_block_number: L2BlockNumber,
    ) -> DalResult<Option<Vec<MessageRoot>>> {
        let records = sqlx::query!(
            r#"
            SELECT message_roots_assigned FROM miniblocks WHERE number = $1
            "#,
            processed_block_number.0 as i64
        )
        .instrument("get_assgigned_roots_assigned")
        .with_arg("processed_block_number", &processed_block_number)
        .fetch_optional(self.storage)
        .await?;

        let message_roots_assigned = records.map(|r| r.message_roots_assigned).unwrap_or(false);
        println!(
            "get_assgigned_roots_or_assign_if_needed {:?} {:?}",
            processed_block_number, message_roots_assigned
        );
        if message_roots_assigned {
            return self.get_assigned_roots(processed_block_number).await;
        } else {
            return self.assign_roots(processed_block_number).await;
        }
    }

    pub async fn get_assigned_roots(
        &mut self,
        processed_block_number: L2BlockNumber,
    ) -> DalResult<Option<Vec<MessageRoot>>> {
        // we get the current max l1 batch number
        // we get the message roots that are
        // processed block numbers matches
        // processed block numbers have the same l1 batch
        let records = sqlx::query!(
            r#"
            WITH l1_batch AS (
                SELECT l1_batch_number
                FROM miniblocks
                WHERE number = $1
                LIMIT 1
            ),
            
            ranked AS (
                SELECT
                    mr.message_root_sides,
                    mr.chain_id,
                    mr.dependency_block_number,
                    ROW_NUMBER()
                        OVER (
                            PARTITION BY mr.chain_id
                            ORDER BY mr.dependency_block_number DESC
                        )
                    AS rn
                FROM message_roots mr
                CROSS JOIN l1_batch lb
                WHERE
                    mr.processed_block_number = $1
                    OR EXISTS (
                        SELECT 1
                        FROM miniblocks mb
                        WHERE
                            mb.number = mr.processed_block_number
                            AND mb.l1_batch_number = lb.l1_batch_number
                    )
            )
            
            SELECT message_root_sides, chain_id, dependency_block_number
            FROM ranked
            WHERE rn <= 5
            ORDER BY chain_id, dependency_block_number DESC;
            "#,
            processed_block_number.0 as i64
        )
        .instrument("get_assigned_roots")
        .with_arg("processed_block_number", &processed_block_number)
        .fetch_all(self.storage)
        .await?;

        let result: Vec<MessageRoot> = records
            .into_iter()
            .map(|record| {
                let block_number = record.dependency_block_number as u32;
                let root = record
                    .message_root_sides
                    .iter()
                    .map(|side| h256_to_u256(H256::from_slice(side)))
                    .collect::<Vec<_>>();

                MessageRoot::new(record.chain_id as u32, block_number, root)
            })
            .collect();

        println!("get_assgined_roots {:?}", result);
        println!("for processed block number 2 {:?}", processed_block_number);

        if result.is_empty() {
            return Ok(None);
        }
        Ok(Some(result))
    }

    pub async fn assign_roots(
        &mut self,
        processed_block_number: L2BlockNumber,
    ) -> DalResult<Option<Vec<MessageRoot>>> {
        // we get the current max l1 batch number
        // we get the message roots that are
        // unprocessed
        // processed block numbers matches
        // processed block numbers have the same l1 batch
        let records = sqlx::query!(
            r#"
            WITH l1_batch AS (
                SELECT l1_batch_number
                FROM miniblocks
                WHERE number = $1
                LIMIT 1
            ),
            
            ranked AS (
                SELECT
                    mr.message_root_sides,
                    mr.chain_id,
                    mr.dependency_block_number,
                    ROW_NUMBER()
                        OVER (
                            PARTITION BY mr.chain_id
                            ORDER BY mr.dependency_block_number DESC
                        )
                    AS rn
                FROM message_roots mr
                CROSS JOIN l1_batch lb
                WHERE
                    mr.processed_block_number IS NULL
                    OR mr.processed_block_number = $1
                    OR EXISTS (
                        SELECT 1
                        FROM miniblocks mb
                        WHERE
                            mb.number = mr.processed_block_number
                            AND mb.l1_batch_number = lb.l1_batch_number
                    )
            )
            
            SELECT message_root_sides, chain_id, dependency_block_number
            FROM ranked
            WHERE rn <= 5
            ORDER BY chain_id, dependency_block_number DESC;
            "#,
            processed_block_number.0 as i64
        )
        .instrument("get_latest_message_root")
        .with_arg("processed_block_number", &processed_block_number)
        .fetch_all(self.storage)
        .await?;

        let result: Vec<MessageRoot> = records
            .into_iter()
            .map(|record| {
                let block_number = record.dependency_block_number as u32;
                let root = record
                    .message_root_sides
                    .iter()
                    .map(|side| h256_to_u256(H256::from_slice(side)))
                    .collect::<Vec<_>>();

                MessageRoot::new(record.chain_id as u32, block_number, root)
            })
            .collect();

        println!("get_latest_message_root {:?}", result);
        println!("for processed block number 1 {:?}", processed_block_number);
        for msg_root in result.clone() {
            self.mark_msg_root_as_processed(msg_root, processed_block_number)
                .await?;
        }
        self.mark_miniblock_as_assigned(processed_block_number)
            .await?;
        if result.is_empty() {
            return Ok(None);
        }
        Ok(Some(result))
    }

    pub async fn mark_msg_root_as_processed(
        &mut self,
        msg_root: MessageRoot,
        processed_block_number: L2BlockNumber,
    ) -> DalResult<()> {
        // Update processed block numbers
        sqlx::query!(
            r#"
            UPDATE MESSAGE_ROOTS
            SET PROCESSED_BLOCK_NUMBER = $1
            WHERE
                CHAIN_ID = $2
                AND DEPENDENCY_BLOCK_NUMBER = $3
                AND PROCESSED_BLOCK_NUMBER IS NULL
            "#,
            processed_block_number.0 as i64,
            msg_root.chain_id as i64,
            msg_root.block_number as i64
        )
        .instrument("update_message_roots_processed_block")
        .with_arg("processed_block_number", &processed_block_number)
        .with_arg("chain_id", &msg_root.chain_id)
        .with_arg("dependency_block_number", &msg_root.block_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn mark_miniblock_as_assigned(
        &mut self,
        processed_block_number: L2BlockNumber,
    ) -> DalResult<()> {
        println!(
            "mark_miniblock_as_assigned block {:?}",
            processed_block_number
        );

        let records = sqlx::query!(
            r#"
            SELECT message_roots_assigned FROM miniblocks WHERE number = $1
            "#,
            processed_block_number.0 as i64
        )
        .instrument("get_assgigned_roots_assigned")
        .with_arg("processed_block_number", &processed_block_number)
        .fetch_optional(self.storage)
        .await?;
        println!("mark_miniblock_as_assigned before {:?}", records);
        let records = sqlx::query!(
            r#"
            SELECT message_roots_assigned, number FROM miniblocks
            "#,
        )
        .instrument("get_assgigned_roots_assigned")
        .fetch_all(self.storage)
        .await?;
        println!("mark_miniblock_as_assigned all {:?}", records);
        sqlx::query!(
            r#"
            UPDATE miniblocks
            SET message_roots_assigned = true
            WHERE number = $1 AND message_roots_assigned = false
            "#,
            processed_block_number.0 as i64
        )
        .instrument("mark_miniblock_as_assigned")
        .with_arg("processed_block_number", &processed_block_number)
        .execute(self.storage)
        .await?;
        let records = sqlx::query!(
            r#"
            SELECT message_roots_assigned FROM miniblocks WHERE number = $1
            "#,
            processed_block_number.0 as i64
        )
        .instrument("get_assgigned_roots_assigned")
        .with_arg("processed_block_number", &processed_block_number)
        .fetch_optional(self.storage)
        .await?;
        println!("mark_miniblock_as_assigned after {:?}", records);
        Ok(())
    }

    pub async fn get_msg_roots_batch(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Vec<MessageRoot>> {
        let block_numbers = sqlx::query!(
            r#"
            SELECT
                number
            FROM miniblocks
            WHERE l1_batch_number = $1
            ORDER BY number ASC
            "#,
            i64::from(batch_number.0)
        )
        .instrument("get_msg_roots_batch")
        .with_arg("batch_number", &batch_number)
        .fetch_all(self.storage)
        .await?;

        println!("get_msg_roots_batch {:?}", block_numbers);
        let mut all_message_roots = Vec::new();
        for block in block_numbers {
            let block_message_roots = self
                .get_dependency_roots(L2BlockNumber(block.number as u32))
                .await?;
            all_message_roots.extend(block_message_roots);
        }

        println!("get_msg_roots_batch {:?}", all_message_roots);
        Ok(all_message_roots)
    }

    pub async fn get_dependency_roots(
        &mut self,
        block_number: L2BlockNumber,
    ) -> DalResult<Vec<MessageRoot>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                MESSAGE_ROOT_SIDES,
                CHAIN_ID,
                DEPENDENCY_BLOCK_NUMBER
            FROM
                MESSAGE_ROOTS
            WHERE
                PROCESSED_BLOCK_NUMBER = $1
            "#,
            i64::from(block_number.0)
        )
        .instrument("get_dependency_roots")
        .with_arg("block_number", &block_number)
        .fetch_all(self.storage)
        .await?;

        let message_roots = rows
            .into_iter()
            .map(|row| {
                let block_number = row.dependency_block_number as u32;
                let root = row
                    .message_root_sides
                    .iter()
                    .map(|side| h256_to_u256(H256::from_slice(side)))
                    .collect::<Vec<_>>();
                let chain_id = row.chain_id as u32;
                MessageRoot::new(chain_id, block_number, root)
            })
            .collect();

        Ok(message_roots)
    }
}
