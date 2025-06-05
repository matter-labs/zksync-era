use itertools::Itertools;
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_multivm::{utils::get_bootloader_max_msg_roots_in_batch, VmVersion};
use zksync_types::{h256_to_u256, InteropRoot, L1BatchNumber, L2BlockNumber, SLChainId, H256};

use crate::Core;

#[derive(Debug)]
pub struct InteropRootDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl InteropRootDal<'_, '_> {
    pub async fn set_interop_root(
        &mut self,
        chain_id: SLChainId,
        number: L1BatchNumber,
        interop_root: &[H256],
        timestamp: u64,
        // proof: BatchAndChainMerklePath,
    ) -> DalResult<()> {
        println!(
            "set_interop_root {:?} {:?} {:?}",
            chain_id.0, number.0, interop_root
        );
        let sides = interop_root
            .iter()
            .map(|root| root.as_bytes().to_vec())
            .collect::<Vec<_>>();
        sqlx::query!(
            r#"
            INSERT INTO interop_roots (
                chain_id, dependency_block_number, interop_root_sides, received_timestamp
            )
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (chain_id, dependency_block_number)
            DO UPDATE SET interop_root_sides = excluded.interop_root_sides;
            "#,
            chain_id.0 as i64,
            i64::from(number.0),
            &sides,
            timestamp as i64
        )
        .instrument("set_interop_root")
        .with_arg("chain_id", &chain_id)
        .with_arg("number", &number)
        .with_arg("interop_root", &interop_root)
        .with_arg("timestamp", &timestamp)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_new_interop_roots(&mut self) -> DalResult<Vec<InteropRoot>> {
        let max_msg_roots_in_batch =
            get_bootloader_max_msg_roots_in_batch(VmVersion::latest()) as i64;
        let records = sqlx::query!(
            r#"
            SELECT *
            FROM interop_roots
            WHERE processed_block_number IS NULL
            ORDER BY received_timestamp, dependency_block_number
            LIMIT $1
            "#,
            max_msg_roots_in_batch
        )
        .instrument("get_new_interop_roots")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|rec| {
            let sides = rec
                .interop_root_sides
                .iter()
                .map(|side| h256_to_u256(H256::from_slice(side)))
                .collect::<Vec<_>>();

            InteropRoot {
                chain_id: rec.chain_id as u32,
                block_number: rec.dependency_block_number as u32,
                sides,
                received_timestamp: rec.received_timestamp as u64,
            }
        })
        .collect();
        Ok(records)
    }

    pub async fn reset_interop_roots_state(
        &mut self,
        l2block_number: L2BlockNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE interop_roots
            SET processed_block_number = NULL
            WHERE processed_block_number = $1
            "#,
            l2block_number.0 as i32
        )
        .instrument("reset_interop_roots_state")
        .fetch_optional(self.storage)
        .await?;
        Ok(())
    }

    pub async fn mark_interop_roots_as_executed(
        &mut self,
        interop_roots: &[InteropRoot],
        l2block_number: L2BlockNumber,
    ) -> DalResult<()> {
        let mut db_transaction = self.storage.start_transaction().await?;
        for root in interop_roots {
            sqlx::query!(
                r#"
                UPDATE interop_roots
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
            .instrument("mark_interop_roots_as_executed")
            .execute(&mut db_transaction)
            .await?;
        }
        db_transaction.commit().await?;
        Ok(())
    }
    pub async fn get_interop_roots(
        &mut self,
        l2block_number: L2BlockNumber,
    ) -> DalResult<Vec<InteropRoot>> {
        let records = sqlx::query!(
            r#"
            SELECT *
            FROM interop_roots
            WHERE processed_block_number = $1
            ORDER BY received_timestamp, dependency_block_number;
            "#,
            l2block_number.0 as i32
        )
        .instrument("get_interop_roots")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|rec| {
            let sides = rec
                .interop_root_sides
                .iter()
                .map(|side| h256_to_u256(H256::from_slice(side)))
                .collect::<Vec<_>>();

            InteropRoot {
                chain_id: rec.chain_id as u32,
                block_number: rec.dependency_block_number as u32,
                sides,
                received_timestamp: rec.received_timestamp as u64,
            }
        })
        .collect();
        Ok(records)
    }

    pub async fn get_interop_roots_batch(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Vec<InteropRoot>> {
        let roots = sqlx::query!(
            r#"
            SELECT *
            FROM interop_roots
            JOIN miniblocks
                ON interop_roots.processed_block_number = miniblocks.number
            WHERE l1_batch_number = $1
            ORDER BY received_timestamp, dependency_block_number;
            "#,
            i64::from(batch_number.0)
        )
        .instrument("get_interop_roots_batch")
        .with_arg("batch_number", &batch_number)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|rec| {
            let sides = rec
                .interop_root_sides
                .iter()
                .map(|side| h256_to_u256(H256::from_slice(side)))
                .collect::<Vec<_>>();

            InteropRoot {
                chain_id: rec.chain_id as u32,
                block_number: rec.dependency_block_number as u32,
                sides,
                received_timestamp: rec.received_timestamp as u64,
            }
        })
        .sorted_by_key(|root| root.received_timestamp)
        .collect();

        Ok(roots)
    }
}