use zksync_types::{basic_fri_types::FinalProofIds, L1BatchNumber};

use crate::{fri_prover_dal::types, StorageProcessor};

#[derive(Debug)]
pub struct FriSchedulerDependencyTrackerDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl FriSchedulerDependencyTrackerDal<'_, '_> {
    pub async fn get_l1_batches_ready_for_queuing(&mut self) -> Vec<i64> {
        sqlx::query!(
            r#"
            UPDATE scheduler_dependency_tracker_fri
            SET
                status = 'queuing'
            WHERE
                l1_batch_number IN (
                    SELECT
                        l1_batch_number
                    FROM
                        scheduler_dependency_tracker_fri
                    WHERE
                        status != 'queued'
                        AND circuit_1_final_prover_job_id IS NOT NULL
                        AND circuit_2_final_prover_job_id IS NOT NULL
                        AND circuit_3_final_prover_job_id IS NOT NULL
                        AND circuit_4_final_prover_job_id IS NOT NULL
                        AND circuit_5_final_prover_job_id IS NOT NULL
                        AND circuit_6_final_prover_job_id IS NOT NULL
                        AND circuit_7_final_prover_job_id IS NOT NULL
                        AND circuit_8_final_prover_job_id IS NOT NULL
                        AND circuit_9_final_prover_job_id IS NOT NULL
                        AND circuit_10_final_prover_job_id IS NOT NULL
                        AND circuit_11_final_prover_job_id IS NOT NULL
                        AND circuit_12_final_prover_job_id IS NOT NULL
                        AND circuit_13_final_prover_job_id IS NOT NULL
                        AND eip_4844_final_prover_job_id_0 IS NOT NULL
                        AND eip_4844_final_prover_job_id_0 IS NOT NULL
                )
            RETURNING
                l1_batch_number;
            "#,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.l1_batch_number)
        .collect()
    }

    pub async fn mark_l1_batches_queued(&mut self, l1_batches: Vec<i64>) {
        sqlx::query!(
            r#"
            UPDATE scheduler_dependency_tracker_fri
            SET
                status = 'queued'
            WHERE
                l1_batch_number = ANY ($1)
            "#,
            &l1_batches[..]
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn set_final_prover_job_id_for_l1_batch(
        &mut self,
        circuit_id: u8,
        final_prover_job_id: u32,
        l1_batch_number: L1BatchNumber,
        // As of 1.4.2, there exist only 2 blobs. Their order matter.
        // `blob_ordering` is used to determine which blob is the first one and which is the second.
        // This will be changed when 1.5.0 will land and there will be a single node proof for blobs.
        blob_ordering: usize,
    ) {
        let query = if circuit_id != types::EIP_4844_CIRCUIT_ID {
            format!(
                r#"
                UPDATE scheduler_dependency_tracker_fri
                SET circuit_{}_final_prover_job_id = $1
                WHERE l1_batch_number = $2
            "#,
                circuit_id
            )
        } else {
            format!(
                r#"
                    UPDATE scheduler_dependency_tracker_fri
                    SET eip_4844_final_prover_job_id_{} = $1
                    WHERE l1_batch_number = $2
                "#,
                blob_ordering,
            )
        };
        sqlx::query(&query)
            .bind(final_prover_job_id as i64)
            .bind(l1_batch_number.0 as i64)
            .execute(self.storage.conn())
            .await
            .unwrap();
    }

    pub async fn get_final_prover_job_ids_for(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> FinalProofIds {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                scheduler_dependency_tracker_fri
            WHERE
                l1_batch_number = $1
            "#,
            l1_batch_number.0 as i64,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .next()
        .map(|row| FinalProofIds {
            node_proof_ids: [
                row.circuit_1_final_prover_job_id.unwrap() as u32,
                row.circuit_2_final_prover_job_id.unwrap() as u32,
                row.circuit_3_final_prover_job_id.unwrap() as u32,
                row.circuit_4_final_prover_job_id.unwrap() as u32,
                row.circuit_5_final_prover_job_id.unwrap() as u32,
                row.circuit_6_final_prover_job_id.unwrap() as u32,
                row.circuit_7_final_prover_job_id.unwrap() as u32,
                row.circuit_8_final_prover_job_id.unwrap() as u32,
                row.circuit_9_final_prover_job_id.unwrap() as u32,
                row.circuit_10_final_prover_job_id.unwrap() as u32,
                row.circuit_11_final_prover_job_id.unwrap() as u32,
                row.circuit_12_final_prover_job_id.unwrap() as u32,
                row.circuit_13_final_prover_job_id.unwrap() as u32,
            ],
            eip_4844_proof_ids: [
                row.eip_4844_final_prover_job_id_0.unwrap() as u32,
                row.eip_4844_final_prover_job_id_1.unwrap() as u32,
            ],
        })
        .unwrap()
    }
}
