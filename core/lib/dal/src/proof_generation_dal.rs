#![doc = include_str!("../doc/ProofGenerationDal.md")]
use std::time::Duration;

use strum::{Display, EnumString};
use zksync_config::configs::proof_data_handler::ProvingMode;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
    utils::pg_interval_from_duration,
};
use zksync_types::L1BatchNumber;

use crate::Core;

#[derive(Debug)]
pub struct ProofGenerationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, EnumString, Display)]
enum ProofGenerationJobStatus {
    #[strum(serialize = "unpicked")]
    Unpicked,
    #[strum(serialize = "picked_by_prover")]
    PickedByProver,
    #[strum(serialize = "generated")]
    Generated,
    #[strum(serialize = "skipped")]
    Skipped,
}

impl ProofGenerationDal<'_, '_> {
    /// Chooses the batch number so that it has all the necessary data to generate the proof
    /// and is not already picked.
    ///
    /// Marks the batch as picked by the prover, preventing it from being picked twice.
    ///
    /// The batch can be unpicked either via a corresponding DAL method, or it is considered
    /// not picked after `processing_timeout` passes.
    pub async fn lock_batch_for_proving(
        &mut self,
        processing_timeout: Duration,
        proving_mode: ProvingMode,
    ) -> DalResult<Option<L1BatchNumber>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);

        // We are picking up the batch for proving by prover cluster if:
        // 1. Global proving mode is prover cluster(no matter what proving mode of batch is set to)
        // 2. Global proving mode is proving network, but proving mode of batch is set to prover cluster
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = 'picked_by_prover',
                proving_mode = 'prover_cluster',
                updated_at = NOW(),
                prover_taken_at = NOW()
            WHERE
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        proof_generation_details
                    LEFT JOIN l1_batches ON l1_batch_number = l1_batches.number
                    WHERE
                        (
                            vm_run_data_blob_url IS NOT NULL
                            AND proof_gen_data_blob_url IS NOT NULL
                            AND l1_batches.hash IS NOT NULL
                            AND l1_batches.aux_data_hash IS NOT NULL
                            AND l1_batches.meta_parameters_hash IS NOT NULL
                            AND status = 'unpicked'
                            AND (
                                $2 = 'prover_cluster'
                                OR proving_mode = 'prover_cluster'
                            )
                        )
                        OR (
                            status = 'picked_by_prover'
                            AND prover_taken_at < NOW() - $1::INTERVAL
                        )
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                )
            RETURNING
            proof_generation_details.l1_batch_number
            "#,
            &processing_timeout,
            &proving_mode.into_string(),
        )
        .instrument("lock_batch_for_proving")
        .with_arg("processing_timeout", &processing_timeout)
        .fetch_optional(self.storage)
        .await?
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(result)
    }

    pub async fn lock_batch_for_proving_network(
        &mut self,
        proving_mode: ProvingMode,
    ) -> DalResult<Option<L1BatchNumber>> {
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = 'picked_by_prover',
                updated_at = NOW(),
                prover_taken_at = NOW()
            WHERE
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        proof_generation_details
                    LEFT JOIN l1_batches ON l1_batch_number = l1_batches.number
                    WHERE
                        (
                            vm_run_data_blob_url IS NOT NULL
                            AND proof_gen_data_blob_url IS NOT NULL
                            AND l1_batches.hash IS NOT NULL
                            AND l1_batches.aux_data_hash IS NOT NULL
                            AND l1_batches.meta_parameters_hash IS NOT NULL
                            AND status = 'unpicked'
                            AND proving_mode = 'proving_network'
                            AND $1 = 'proving_network'
                        )
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                )
            RETURNING
            proof_generation_details.l1_batch_number
            "#,
            &proving_mode.into_string(),
        )
        .instrument("lock_batch_for_proving_network")
        .fetch_optional(self.storage)
        .await?
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(result)
    }

    pub async fn get_latest_proven_batch(&mut self) -> DalResult<L1BatchNumber> {
        let result = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                proof_generation_details
            WHERE
                proof_blob_url IS NOT NULL
            ORDER BY
                l1_batch_number DESC
            LIMIT
                1
            "#,
        )
        .instrument("get_available batch")
        .fetch_one(self.storage)
        .await?
        .l1_batch_number as u32;

        Ok(L1BatchNumber(result))
    }

    /// Marks a previously locked batch as 'unpicked', allowing it to be picked without having
    /// to wait for the processing timeout.
    pub async fn unlock_batch(&mut self, l1_batch_number: L1BatchNumber) -> DalResult<()> {
        let batch_number = i64::from(l1_batch_number.0);
        sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = 'unpicked',
                updated_at = NOW()
            WHERE
                l1_batch_number = $1
            "#,
            batch_number,
        )
        .instrument("unlock_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn save_proof_artifacts_metadata(
        &mut self,
        batch_number: L1BatchNumber,
        proof_blob_url: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = 'generated',
                proof_blob_url = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            proof_blob_url,
            batch_number
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("proof_blob_url", &proof_blob_url)
            .with_arg("l1_batch_number", &batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot save proof_blob_url for a batch number {} that does not exist",
                batch_number
            ));
            return Err(err);
        }

        Ok(())
    }

    pub async fn save_vm_runner_artifacts_metadata(
        &mut self,
        batch_number: L1BatchNumber,
        vm_run_data_blob_url: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                vm_run_data_blob_url = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            vm_run_data_blob_url,
            batch_number
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("vm_run_data_blob_url", &vm_run_data_blob_url)
            .with_arg("l1_batch_number", &batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot save vm_run_data_blob_url for a batch number {} that does not exist",
                batch_number
            ));
            return Err(err);
        }

        Ok(())
    }

    pub async fn save_merkle_paths_artifacts_metadata(
        &mut self,
        batch_number: L1BatchNumber,
        proof_gen_data_blob_url: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                proof_gen_data_blob_url = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            proof_gen_data_blob_url,
            batch_number
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("proof_gen_data_blob_url", &proof_gen_data_blob_url)
            .with_arg("l1_batch_number", &batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot save proof_gen_data_blob_url for a batch number {} that does not exist",
                batch_number
            ));
            return Err(err);
        }

        Ok(())
    }

    /// The caller should ensure that `l1_batch_number` exists in the database.
    pub async fn insert_proof_generation_details(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<()> {
        let result = sqlx::query!(
            r#"
            INSERT INTO
            proof_generation_details (l1_batch_number, status, created_at, updated_at)
            VALUES
            ($1, 'unpicked', NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            i64::from(l1_batch_number.0),
        )
        .instrument("insert_proof_generation_details")
        .with_arg("l1_batch_number", &l1_batch_number)
        .report_latency()
        .execute(self.storage)
        .await?;

        if result.rows_affected() == 0 {
            // Not an error: we may call `insert_proof_generation_details()` from multiple full trees instantiated
            // for the same node. Unlike tree data, we don't particularly care about correspondence of `proof_gen_data_blob_url` across calls,
            // so just log this fact and carry on.
            tracing::debug!("L1 batch #{l1_batch_number}: proof generation data wasn't updated as it's already present");
        }
        Ok(())
    }

    pub async fn mark_proof_generation_job_as_skipped(
        &mut self,
        block_number: L1BatchNumber,
    ) -> DalResult<()> {
        let status = ProofGenerationJobStatus::Skipped.to_string();
        let l1_batch_number = i64::from(block_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            status,
            l1_batch_number
        );
        let instrumentation = Instrumented::new("mark_proof_generation_job_as_skipped")
            .with_arg("status", &status)
            .with_arg("l1_batch_number", &l1_batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot mark proof as skipped because batch number {} does not exist",
                l1_batch_number
            ));
            return Err(err);
        }

        Ok(())
    }

    pub async fn get_oldest_unpicked_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                proof_generation_details
            WHERE
                status = 'unpicked'
            ORDER BY
                l1_batch_number ASC
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(result)
    }

    pub async fn get_oldest_not_generated_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                proof_generation_details
            WHERE
                status NOT IN ('generated', 'skipped')
            ORDER BY
                l1_batch_number ASC
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        block::L1BatchTreeData, commitment::L1BatchCommitmentArtifacts, ProtocolVersion, H256,
    };

    use super::*;
    use crate::{tests::create_l1_batch_header, ConnectionPool, CoreDal};

    #[tokio::test]
    async fn proof_generation_workflow() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_mock_l1_batch(&create_l1_batch_header(1))
            .await
            .unwrap();

        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, None);

        conn.proof_generation_dal()
            .insert_proof_generation_details(L1BatchNumber(1))
            .await
            .unwrap();

        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, Some(L1BatchNumber(1)));

        // Calling the method multiple times should work fine.
        conn.proof_generation_dal()
            .insert_proof_generation_details(L1BatchNumber(1))
            .await
            .unwrap();
        conn.proof_generation_dal()
            .save_vm_runner_artifacts_metadata(L1BatchNumber(1), "vm_run")
            .await
            .unwrap();
        conn.proof_generation_dal()
            .save_merkle_paths_artifacts_metadata(L1BatchNumber(1), "data")
            .await
            .unwrap();
        conn.blocks_dal()
            .save_l1_batch_tree_data(
                L1BatchNumber(1),
                &L1BatchTreeData {
                    hash: H256::zero(),
                    rollup_last_leaf_index: 123,
                },
            )
            .await
            .unwrap();
        conn.blocks_dal()
            .save_l1_batch_commitment_artifacts(
                L1BatchNumber(1),
                &L1BatchCommitmentArtifacts::default(),
            )
            .await
            .unwrap();

        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, Some(L1BatchNumber(1)));

        let picked_l1_batch = conn
            .proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, ProvingMode::ProverCluster)
            .await
            .unwrap();
        assert_eq!(picked_l1_batch, Some(L1BatchNumber(1)));
        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, None);

        // Check that we can unlock the batch and then pick it again.
        conn.proof_generation_dal()
            .unlock_batch(L1BatchNumber(1))
            .await
            .unwrap();
        let picked_l1_batch = conn
            .proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, ProvingMode::ProverCluster)
            .await
            .unwrap();
        assert_eq!(picked_l1_batch, Some(L1BatchNumber(1)));

        // Check that with small enough processing timeout, the L1 batch can be picked again
        let picked_l1_batch = conn
            .proof_generation_dal()
            .lock_batch_for_proving(Duration::ZERO, ProvingMode::ProverCluster)
            .await
            .unwrap();
        assert_eq!(picked_l1_batch, Some(L1BatchNumber(1)));

        conn.proof_generation_dal()
            .save_proof_artifacts_metadata(L1BatchNumber(1), "proof")
            .await
            .unwrap();

        let picked_l1_batch = conn
            .proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, ProvingMode::ProverCluster)
            .await
            .unwrap();
        assert_eq!(picked_l1_batch, None);
        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, None);
    }
}
