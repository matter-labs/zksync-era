use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::{pubdata_da::DataAvailabilityBlob, L1BatchNumber};

use crate::{
    models::storage_data_availability::{L1BatchDA, StorageDABlob},
    Core,
};

#[derive(Debug)]
pub struct DataAvailabilityDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl DataAvailabilityDal<'_, '_> {
    /// Inserts the blob_id for the given L1 batch. If the blob_id is already present,
    /// verifies that it matches the one provided in the function arguments
    /// (preventing the same L1 batch from being stored twice)
    pub async fn insert_l1_batch_da(
        &mut self,
        number: L1BatchNumber,
        blob_id: &str,
        sent_at: chrono::NaiveDateTime,
    ) -> DalResult<()> {
        let update_result = sqlx::query!(
            r#"
            INSERT INTO
                data_availability (l1_batch_number, blob_id, sent_at, created_at, updated_at)
            VALUES
                ($1, $2, $3, NOW(), NOW())
            "#,
            i64::from(number.0),
            blob_id,
            sent_at,
        )
        .instrument("insert_l1_batch_da")
        .with_arg("number", &number)
        .with_arg("blob_id", &blob_id)
        .report_latency()
        .execute(self.storage)
        .await?;

        if update_result.rows_affected() == 0 {
            tracing::debug!(
                "L1 batch #{number}: DA blob_id wasn't updated as it's already present"
            );

            let instrumentation = Instrumented::new("get_matching_batch_da_blob_id")
                .with_arg("number", &number)
                .with_arg("blob_id", &blob_id);

            // Batch was already processed. Verify that existing DA blob_id matches
            let query = sqlx::query!(
                r#"
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    data_availability
                WHERE
                    l1_batch_number = $1
                    AND blob_id = $2
                "#,
                i64::from(number.0),
                blob_id,
            );

            let matched: i64 = instrumentation
                .clone()
                .with(query)
                .report_latency()
                .fetch_one(self.storage)
                .await?
                .count;

            if matched != 1 {
                let err = instrumentation.constraint_error(anyhow::anyhow!(
                    "Error storing DA blob id. DA blob_id {blob_id} for L1 batch #{number} does not match the expected value"
                ));
                return Err(err);
            }
        }
        Ok(())
    }

    /// Saves the inclusion data for the given L1 batch. If the inclusion data is already present,
    /// verifies that it matches the one provided in the function arguments
    /// (meaning that the inclusion data corresponds to the same DA blob)
    pub async fn save_l1_batch_inclusion_data(
        &mut self,
        number: L1BatchNumber,
        da_inclusion_data: &[u8],
    ) -> DalResult<()> {
        let update_result = sqlx::query!(
            r#"
            UPDATE data_availability
            SET
                inclusion_data = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
                AND inclusion_data IS NULL
            "#,
            da_inclusion_data,
            i64::from(number.0),
        )
        .instrument("save_l1_batch_da_data")
        .with_arg("number", &number)
        .report_latency()
        .execute(self.storage)
        .await?;

        if update_result.rows_affected() == 0 {
            tracing::debug!("L1 batch #{number}: DA data wasn't updated as it's already present");

            let instrumentation =
                Instrumented::new("get_matching_batch_da_data").with_arg("number", &number);

            // Batch was already processed. Verify that existing DA data matches
            let query = sqlx::query!(
                r#"
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    data_availability
                WHERE
                    l1_batch_number = $1
                    AND inclusion_data = $2
                "#,
                i64::from(number.0),
                da_inclusion_data,
            );

            let matched: i64 = instrumentation
                .clone()
                .with(query)
                .report_latency()
                .fetch_one(self.storage)
                .await?
                .count;

            if matched != 1 {
                let err = instrumentation.constraint_error(anyhow::anyhow!(
                    "Error storing DA inclusion data. DA data for L1 batch #{number} does not match the one provided before"
                ));
                return Err(err);
            }
        }
        Ok(())
    }

    /// Assumes that the L1 batches are sorted by number, and returns the first one that is ready for DA dispatch.
    pub async fn get_first_da_blob_awaiting_inclusion(
        &mut self,
    ) -> DalResult<Option<DataAvailabilityBlob>> {
        Ok(sqlx::query_as!(
            StorageDABlob,
            r#"
            SELECT
                l1_batch_number,
                blob_id,
                inclusion_data,
                sent_at
            FROM
                data_availability
            WHERE
                inclusion_data IS NULL
            ORDER BY
                l1_batch_number
            LIMIT
                1
            "#,
        )
        .instrument("get_first_da_blob_awaiting_inclusion")
        .fetch_optional(self.storage)
        .await?
        .map(DataAvailabilityBlob::from))
    }

    /// Fetches the pubdata and `l1_batch_number` for the L1 batches that are ready for DA dispatch.
    pub async fn get_ready_for_da_dispatch_l1_batches(
        &mut self,
        limit: usize,
    ) -> DalResult<Vec<L1BatchDA>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                number,
                pubdata_input
            FROM
                l1_batches
                LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
            WHERE
                eth_commit_tx_id IS NULL
                AND number != 0
                AND data_availability.blob_id IS NULL
                AND pubdata_input IS NOT NULL
            ORDER BY
                number
            LIMIT
                $1
            "#,
            limit as i64,
        )
        .instrument("get_ready_for_da_dispatch_l1_batches")
        .with_arg("limit", &limit)
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| L1BatchDA {
                // `unwrap` is safe here because we have a `WHERE` clause that filters out `NULL` values
                pubdata: row.pubdata_input.unwrap(),
                l1_batch_number: L1BatchNumber(row.number as u32),
            })
            .collect())
    }
}
