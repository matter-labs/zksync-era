use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{InteropRoot, L1BatchNumber, L2BlockNumber, L2ChainId, SLChainId, H256};

use crate::Core;

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct StorageInteropRoot {
    pub chain_id: i64,
    pub dependency_block_number: i64,
    pub interop_root_sides: Vec<Vec<u8>>,
}

impl TryFrom<StorageInteropRoot> for InteropRoot {
    type Error = sqlx::Error;

    fn try_from(rec: StorageInteropRoot) -> Result<Self, Self::Error> {
        let sides = rec
            .interop_root_sides
            .iter()
            .map(|side| H256::from_slice(side))
            .collect::<Vec<_>>();

        Ok(Self {
            chain_id: L2ChainId::new(rec.chain_id as u64).unwrap(),
            block_number: rec.dependency_block_number as u32,
            sides,
        })
    }
}

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
    ) -> DalResult<()> {
        let sides = interop_root
            .iter()
            .map(|root| root.as_bytes().to_vec())
            .collect::<Vec<_>>();
        sqlx::query!(
            r#"
            INSERT INTO interop_roots (
                chain_id, dependency_block_number, interop_root_sides
            )
            VALUES ($1, $2, $3)
            ON CONFLICT (chain_id, dependency_block_number)
            DO UPDATE SET interop_root_sides = excluded.interop_root_sides;
            "#,
            chain_id.0 as i64,
            i64::from(number.0),
            &sides,
        )
        .instrument("set_interop_root")
        .with_arg("chain_id", &chain_id)
        .with_arg("number", &number)
        .with_arg("interop_root", &interop_root)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_new_interop_roots(
        &mut self,
        number_of_roots: usize,
    ) -> DalResult<Vec<InteropRoot>> {
        let records = sqlx::query_as!(
            StorageInteropRoot,
            r#"
            SELECT
                interop_roots.chain_id,
                interop_roots.dependency_block_number,
                interop_roots.interop_root_sides
            FROM interop_roots
            WHERE processed_block_number IS NULL
            ORDER BY chain_id, dependency_block_number DESC
            LIMIT $1
            "#,
            number_of_roots as i64
        )
        .try_map(InteropRoot::try_from)
        .instrument("get_new_interop_roots")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .collect();
        Ok(records)
    }

    pub async fn reset_interop_roots_state(
        &mut self,
        last_correct_l2_block: L2BlockNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE interop_roots
            SET processed_block_number = NULL
            WHERE processed_block_number > $1
            "#,
            last_correct_l2_block.0 as i32
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
        insert_roots: bool,
    ) -> DalResult<()> {
        if insert_roots {
            self.inserted_executed_interop_roots(interop_roots, l2block_number)
                .await
        } else {
            self.update_interop_roots_as_executed(interop_roots, l2block_number)
                .await
        }
    }
    pub async fn inserted_executed_interop_roots(
        &mut self,
        interop_roots: &[InteropRoot],
        l2block_number: L2BlockNumber,
    ) -> DalResult<()> {
        let mut db_transaction = self.storage.start_transaction().await?;
        for root in interop_roots {
            let sides = root
                .sides
                .iter()
                .cloned()
                .map(|root| root.as_bytes().to_vec())
                .collect::<Vec<_>>();

            // There can be interop root in the DB in case of block rollback or if the DB was restored from a dump.
            sqlx::query!(
                r#"
                INSERT INTO interop_roots
                (
                    chain_id,
                    dependency_block_number,
                    interop_root_sides,
                    processed_block_number
                )
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (chain_id, dependency_block_number)
                DO UPDATE SET interop_root_sides = excluded.interop_root_sides,
                processed_block_number = excluded.processed_block_number;
                "#,
                root.chain_id.as_u64() as i64,
                root.block_number as i32,
                &sides,
                l2block_number.0 as i32,
            )
            .instrument("mark_interop_roots_as_executed")
            .execute(&mut db_transaction)
            .await?;
        }
        db_transaction.commit().await?;
        Ok(())
    }

    pub async fn update_interop_roots_as_executed(
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
                    AND dependency_block_number = $3
                "#,
                l2block_number.0 as i32,
                root.chain_id.as_u64() as i64,
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
        let records = sqlx::query_as!(
            StorageInteropRoot,
            r#"
            SELECT
                interop_roots.chain_id,
                interop_roots.dependency_block_number,
                interop_roots.interop_root_sides
            FROM interop_roots
            WHERE processed_block_number = $1
            ORDER BY chain_id, dependency_block_number DESC;
            "#,
            l2block_number.0 as i32
        )
        .try_map(InteropRoot::try_from)
        .instrument("get_interop_roots")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .collect();
        Ok(records)
    }

    pub async fn get_interop_roots_batch(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Vec<InteropRoot>> {
        let roots = sqlx::query_as!(
            StorageInteropRoot,
            r#"
            SELECT
                interop_roots.chain_id,
                interop_roots.dependency_block_number,
                interop_roots.interop_root_sides
            FROM interop_roots
            JOIN miniblocks
                ON interop_roots.processed_block_number = miniblocks.number
            WHERE l1_batch_number = $1
            ORDER BY chain_id, dependency_block_number DESC;
            "#,
            i64::from(batch_number.0)
        )
        .try_map(InteropRoot::try_from)
        .instrument("get_interop_roots_batch")
        .with_arg("batch_number", &batch_number)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .collect();

        Ok(roots)
    }
}
