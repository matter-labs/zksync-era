use std::time::Duration;

use zksync_types::L1BatchNumber;

use crate::time_utils::pg_interval_from_duration;
use crate::{SqlxError, StorageProcessor};

#[derive(Debug)]
pub struct ProofGenerationDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl ProofGenerationDal<'_, '_> {
    pub async fn get_next_block_to_be_proven(
        &mut self,
        processing_timeout: Duration,
    ) -> Option<L1BatchNumber> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let result: Option<L1BatchNumber> = sqlx::query!(
            "UPDATE proof_generation_details \
             SET status = 'picked_by_prover', updated_at = now(), prover_taken_at = now() \
             WHERE l1_batch_number = ( \
                 SELECT l1_batch_number \
                 FROM proof_generation_details \
                 WHERE status = 'ready_to_be_proven' \
                 OR (status = 'picked_by_prover' AND prover_taken_at < now() - $1::interval) \
                 ORDER BY l1_batch_number ASC \
                 LIMIT 1 \
                 FOR UPDATE \
                 SKIP LOCKED \
             ) \
             RETURNING proof_generation_details.l1_batch_number",
            &processing_timeout,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        result
    }

    pub async fn save_proof_artifacts_metadata(
        &mut self,
        block_number: L1BatchNumber,
        proof_blob_url: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query!(
            "UPDATE proof_generation_details \
             SET status='generated', proof_blob_url = $1, updated_at = now() \
             WHERE l1_batch_number = $2",
            proof_blob_url,
            block_number.0 as i64,
        )
        .execute(self.storage.conn())
        .await?
        .rows_affected()
        .eq(&1)
        .then_some(())
        .ok_or(sqlx::Error::RowNotFound)
    }
}
