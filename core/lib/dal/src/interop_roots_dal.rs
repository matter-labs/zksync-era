use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{
    h256_to_u256, interop_root::MessageRoot, L1BatchNumber, L2BlockNumber, SLChainId, H256,
};

use crate::Core;

#[derive(Debug)]
pub struct MessageRootDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl MessageRootDal<'_, '_> {
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
            INSERT INTO message_roots (
                chain_id, dependency_block_number, message_root_sides
            )
            VALUES ($1, $2, $3)
            ON CONFLICT (chain_id, dependency_block_number)
            DO UPDATE SET message_root_sides = excluded.message_root_sides;
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

    pub async fn get_new_message_roots(&mut self) -> DalResult<Vec<MessageRoot>> {
        // kl todo only load MAX_MSG_ROOTS_IN_BATCH message roots
        let records = sqlx::query!(
            r#"
            SELECT *
            FROM message_roots
            WHERE processed_block_number IS NULL;
            "#,
        )
        .instrument("get_new_message_roots")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|rec| {
            let sides = rec
                .message_root_sides
                .iter()
                .map(|side| h256_to_u256(H256::from_slice(side)))
                .collect::<Vec<_>>();

            MessageRoot {
                chain_id: rec.chain_id as u32,
                block_number: rec.dependency_block_number as u32,
                sides,
            }
        })
        .collect();
        Ok(records)
    }

    pub async fn reset_message_roots_state(
        &mut self,
        l2block_number: L2BlockNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE message_roots
            SET processed_block_number = NULL
            WHERE processed_block_number = $1
            "#,
            l2block_number.0 as i32
        )
        .instrument("reset_message_roots_state")
        .fetch_optional(self.storage)
        .await?;
        Ok(())
    }

    pub async fn mark_message_roots_as_executed(
        &mut self,
        message_roots: &[MessageRoot],
        l2block_number: L2BlockNumber,
    ) -> DalResult<()> {
        let mut db_transaction = self.storage.start_transaction().await?;
        for root in message_roots {
            sqlx::query!(
                r#"
                UPDATE message_roots
                SET processed_block_number = $1
                WHERE
                    chain_id = $2
                    AND processed_block_number IS NULL
                    AND dependency_block_number <= $3
                "#,
                l2block_number.0 as i32,
                root.chain_id as i32,
                root.block_number as i32
            )
            .instrument("mark_message_roots_as_executed")
            .execute(&mut db_transaction)
            .await?;
        }
        db_transaction.commit().await?;
        Ok(())
    }
    pub async fn get_message_roots(
        &mut self,
        l2block_number: L2BlockNumber,
    ) -> DalResult<Vec<MessageRoot>> {
        let records = sqlx::query!(
            r#"
            SELECT *
            FROM message_roots
            WHERE processed_block_number = $1
            "#,
            l2block_number.0 as i32
        )
        .instrument("get_message_roots")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|rec| {
            let sides = rec
                .message_root_sides
                .iter()
                .map(|side| h256_to_u256(H256::from_slice(side)))
                .collect::<Vec<_>>();

            MessageRoot {
                chain_id: rec.chain_id as u32,
                block_number: rec.dependency_block_number as u32,
                sides,
            }
        })
        .collect();
        Ok(records)
    }

    pub async fn get_msg_roots_batch(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Vec<MessageRoot>> {
        let roots = sqlx::query!(
            r#"
            SELECT *
            FROM message_roots
            JOIN miniblocks
                ON message_roots.processed_block_number = miniblocks.number
            WHERE l1_batch_number = $1
            "#,
            i64::from(batch_number.0)
        )
        .instrument("get_msg_roots_batch")
        .with_arg("batch_number", &batch_number)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|rec| {
            let sides = rec
                .message_root_sides
                .iter()
                .map(|side| h256_to_u256(H256::from_slice(side)))
                .collect::<Vec<_>>();

            MessageRoot {
                chain_id: rec.chain_id as u32,
                block_number: rec.dependency_block_number as u32,
                sides,
            }
        })
        .collect();
        Ok(roots)
    }
}
