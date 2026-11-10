use std::{num::NonZeroU32, time::Duration};

use sqlx::QueryBuilder;
use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::InstrumentExt,
    utils::pg_interval_from_duration,
};
use zksync_types::{
    aggregated_operations::{AggregatedActionType, L1BatchAggregatedActionType},
    L1BatchNumber, H256,
};

use crate::{Core, CoreDal};

#[derive(Debug)]
pub struct EthProofManagerDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, Clone)]
pub enum EthProofManagerStatus {
    Preparing,
    Submitting,
    Fallbacked,
    Unpicked,
    Sent,
    Acknowledged,
    Proven,
    Validated,
}

impl EthProofManagerStatus {
    pub fn as_str(&self) -> &str {
        match self {
            EthProofManagerStatus::Preparing => "preparing",
            EthProofManagerStatus::Submitting => "submitting",
            EthProofManagerStatus::Fallbacked => "fallbacked",
            EthProofManagerStatus::Unpicked => "unpicked",
            EthProofManagerStatus::Sent => "sent",
            EthProofManagerStatus::Acknowledged => "acknowledged",
            EthProofManagerStatus::Proven => "proven",
            EthProofManagerStatus::Validated => "validated",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ProvingNetwork {
    None,
    Lagrange,
    Fermah,
}

impl ProvingNetwork {
    pub fn as_str(&self) -> &str {
        match self {
            ProvingNetwork::None => "none",
            ProvingNetwork::Lagrange => "lagrange",
            ProvingNetwork::Fermah => "fermah",
        }
    }
}

impl EthProofManagerDal<'_, '_> {
    /// Hands unpicked proving-network batches whose number is not a multiple of `batch_step`
    /// to the prover cluster.
    pub async fn route_unsampled_batches_to_prover_cluster(
        &mut self,
        batch_step: NonZeroU32,
    ) -> DalResult<u64> {
        let result = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET proving_mode = 'prover_cluster', updated_at = NOW()
            WHERE
                status = 'unpicked'
                AND proving_mode = 'proving_network'
                AND l1_batch_number % $1 != 0
            "#,
            i64::from(batch_step.get()),
        )
        .instrument("route_unsampled_batches_to_prover_cluster")
        .with_arg("batch_step", &batch_step)
        .execute(self.storage)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn lock_batch_for_proving_network_and_prepare(
        &mut self,
        batch_step: NonZeroU32,
    ) -> DalResult<Option<L1BatchNumber>> {
        let mut transaction = self.storage.start_transaction().await?;

        let batch = sqlx::query!(
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
                            AND l1_batch_number % $1 = 0
                        )
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                )
            RETURNING
            proof_generation_details.l1_batch_number
            "#,
            i64::from(batch_step.get()),
        )
        .instrument("lock_batch_for_proving_network_and_prepare#lock_proof")
        .fetch_optional(&mut transaction)
        .await?
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        if let Some(batch_number) = batch {
            sqlx::query!(
                r#"
                INSERT INTO eth_proof_manager (
                    l1_batch_number, status, created_at, witness_inputs_url, updated_at
                )
                VALUES ($1, $2, NOW(), NULL, NOW())
                "#,
                i64::from(batch_number.0),
                EthProofManagerStatus::Preparing.as_str(),
            )
            .instrument("lock_batch_for_proving_network_and_prepare#insert_eth_row")
            .with_arg("batch_number", &batch_number)
            .execute(&mut transaction)
            .await?;
        }

        transaction.commit().await?;

        Ok(batch)
    }

    pub async fn insert_batch(
        &mut self,
        batch_number: L1BatchNumber,
        witness_inputs_url: &str,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO eth_proof_manager (
                l1_batch_number, status, created_at, witness_inputs_url, updated_at
            )
            VALUES ($1, $2, NOW(), $3, NOW())
            "#,
            i64::from(batch_number.0),
            EthProofManagerStatus::Unpicked.as_str(),
            witness_inputs_url
        )
        .instrument("insert_batch")
        .with_arg("batch_number", &batch_number)
        .with_arg("status", &EthProofManagerStatus::Unpicked.as_str())
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn mark_batch_as_submitting(
        &mut self,
        batch_number: L1BatchNumber,
        witness_inputs_url: &str,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                witness_inputs_url = $2,
                submission_started_at = NOW(),
                updated_at = NOW(),
                status = $3
            WHERE l1_batch_number = $1
            "#,
            i64::from(batch_number.0),
            witness_inputs_url,
            EthProofManagerStatus::Submitting.as_str(),
        )
        .instrument("mark_batch_as_submitting")
        .with_arg("batch_number", &batch_number)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    /// Ignores acknowledgments for fallbacked, proven, or validated batches.
    pub async fn acknowledge_batch(
        &mut self,
        batch_number: L1BatchNumber,
        assigned_to: ProvingNetwork,
    ) -> DalResult<()> {
        let result = sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                status = $2,
                updated_at = NOW(),
                assigned_to = $3
            WHERE
                l1_batch_number = $1
                AND status NOT IN ($4, $5, $6)
            "#,
            i64::from(batch_number.0),
            EthProofManagerStatus::Acknowledged.as_str(),
            assigned_to.as_str(),
            EthProofManagerStatus::Fallbacked.as_str(),
            EthProofManagerStatus::Proven.as_str(),
            EthProofManagerStatus::Validated.as_str(),
        )
        .instrument("acknowledge_batch")
        .with_arg("batch_number", &batch_number)
        .with_arg("assigned_to", &assigned_to)
        .execute(self.storage)
        .await?;

        if result.rows_affected() == 0 {
            tracing::info!(
                "Batch {batch_number} already moved past acknowledgment, ignoring the event"
            );
        }

        Ok(())
    }

    /// Records submission metadata without regressing acknowledged status.
    pub async fn mark_batch_as_sent(
        &mut self,
        batch_number: L1BatchNumber,
        tx_hash: H256,
    ) -> DalResult<()> {
        let mut transaction = self.storage.start_transaction().await?;

        let result = sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                submit_proof_request_tx_hash = $2,
                submit_proof_request_tx_sent_at
                = COALESCE(submit_proof_request_tx_sent_at, NOW()),
                updated_at = NOW()
            WHERE
                l1_batch_number = $1
                AND status NOT IN ($3, $4, $5)
            "#,
            i64::from(batch_number.0),
            tx_hash.as_bytes(),
            EthProofManagerStatus::Fallbacked.as_str(),
            EthProofManagerStatus::Proven.as_str(),
            EthProofManagerStatus::Validated.as_str(),
        )
        .instrument("mark_batch_as_sent#record_submission")
        .with_arg("batch_number", &batch_number)
        .with_arg("tx_hash", &tx_hash)
        .execute(&mut transaction)
        .await?;

        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET status = $2
            WHERE
                l1_batch_number = $1
                AND status NOT IN ($3, $4, $5, $6)
            "#,
            i64::from(batch_number.0),
            EthProofManagerStatus::Sent.as_str(),
            EthProofManagerStatus::Acknowledged.as_str(),
            EthProofManagerStatus::Fallbacked.as_str(),
            EthProofManagerStatus::Proven.as_str(),
            EthProofManagerStatus::Validated.as_str(),
        )
        .instrument("mark_batch_as_sent#advance_status")
        .with_arg("batch_number", &batch_number)
        .execute(&mut transaction)
        .await?;

        transaction.commit().await?;

        if result.rows_affected() == 0 {
            tracing::info!(
                "Batch {batch_number} already moved past request submission, \
                 not recording tx {tx_hash:?}"
            );
        }

        Ok(())
    }

    pub async fn mark_batch_as_validated(
        &mut self,
        batch_number: L1BatchNumber,
        tx_hash: H256,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                validated_proof_request_tx_hash = $2,
                validated_proof_request_tx_sent_at = NOW(),
                updated_at = NOW(),
                status = $3
            WHERE l1_batch_number = $1
            "#,
            i64::from(batch_number.0),
            tx_hash.as_bytes(),
            EthProofManagerStatus::Validated.as_str()
        )
        .instrument("mark_batch_as_validated")
        .with_arg("batch_number", &batch_number)
        .with_arg("tx_hash", &tx_hash)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    /// Marks a batch as reported when the contract already holds a validation result.
    ///
    /// Leaves `validated_proof_request_tx_hash` NULL because we do not know which
    /// transaction settled the request — the status alone takes the row out of
    /// [`Self::get_batch_to_send_validation_result`]. Without this, a request that reached
    /// `Validated` on chain without us recording the hash reverts once per backoff
    /// interval forever.
    ///
    /// The settling transaction is usually someone else's, but it can be one of ours that
    /// we lost track of: a send that exhausted its attempts unconfirmed, then mined, so
    /// the next round reverted because of it. `confirmed_broadcast_hash` reconciles that
    /// within one send, not across rounds, so the audit link is dropped in that case. The
    /// hash is recoverable from the `ProofValidationResult` event if that ever matters —
    /// nothing consumes that event today.
    pub async fn mark_batch_validation_already_recorded(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                status = $2,
                updated_at = NOW()
            WHERE
                l1_batch_number = $1
                AND validated_proof_request_tx_hash IS NULL
            "#,
            i64::from(batch_number.0),
            EthProofManagerStatus::Validated.as_str(),
        )
        .instrument("mark_batch_validation_already_recorded")
        .with_arg("batch_number", &batch_number)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    /// Preserves existing verdicts and rewards, and keeps fallbacked or validated status.
    pub async fn mark_batch_as_proven(
        &mut self,
        batch_number: L1BatchNumber,
        proof_validation_result: bool,
        requested_reward: u64,
    ) -> DalResult<()> {
        let mut transaction = self.storage.start_transaction().await?;

        let result = sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                proof_validation_result = $2,
                requested_reward = $3,
                updated_at = NOW()
            WHERE
                l1_batch_number = $1
                AND proof_validation_result IS NULL
            "#,
            i64::from(batch_number.0),
            proof_validation_result,
            requested_reward as i64,
        )
        .instrument("mark_batch_as_proven#record_verdict")
        .with_arg("batch_number", &batch_number)
        .execute(&mut transaction)
        .await?;

        if result.rows_affected() == 0 {
            tracing::info!(
                "Batch {batch_number} already has a validation result, \
                 keeping it over the replayed {proof_validation_result}"
            );
        }

        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                status = $2,
                updated_at = NOW()
            WHERE
                l1_batch_number = $1
                AND status NOT IN ($3, $4)
            "#,
            i64::from(batch_number.0),
            EthProofManagerStatus::Proven.as_str(),
            EthProofManagerStatus::Fallbacked.as_str(),
            EthProofManagerStatus::Validated.as_str(),
        )
        .instrument("mark_batch_as_proven#advance_status")
        .with_arg("batch_number", &batch_number)
        .execute(&mut transaction)
        .await?;

        transaction.commit().await?;

        Ok(())
    }

    pub async fn get_batch_to_send(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let batch: Option<L1BatchNumber> = sqlx::query!(
            r#"
            SELECT l1_batch_number, submit_proof_request_tx_hash
            FROM eth_proof_manager
            WHERE status = $1
            ORDER BY created_at ASC
            LIMIT 1
            "#,
            EthProofManagerStatus::Unpicked.as_str()
        )
        .instrument("get_batch_to_send")
        .fetch_optional(self.storage)
        .await?
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(batch)
    }

    /// Picks an unreported verdict, spacing retries of the same batch by `retry_backoff`
    /// and giving up on it after `max_attempts`.
    pub async fn get_batch_to_send_validation_result(
        &mut self,
        max_attempts: u64,
        retry_backoff: Duration,
    ) -> anyhow::Result<Option<(L1BatchNumber, bool)>> {
        let batch_stats = self
            .storage
            .eth_sender_dal()
            .get_eth_all_blocks_stat()
            .await?;

        // Without an L1 publication, only invalid verdicts are eligible.
        let latest_proven_batch = batch_stats
            .mined
            .iter()
            .find(|(op_type, _)| {
                op_type
                    == &AggregatedActionType::L1Batch(
                        L1BatchAggregatedActionType::PublishProofOnchain,
                    )
            })
            .map(|(_, block_number)| i64::from(*block_number))
            .unwrap_or(-1);

        let retry_backoff = pg_interval_from_duration(retry_backoff);

        let result: Option<(L1BatchNumber, Option<bool>)> = sqlx::query!(
            r#"
            UPDATE eth_proof_manager e
            SET
                validation_tx_attempts = e.validation_tx_attempts + 1,
                validation_tx_last_attempt_at = NOW(),
                updated_at = NOW()
            WHERE
                e.l1_batch_number = (
                    SELECT e2.l1_batch_number
                    FROM eth_proof_manager e2
                    WHERE
                        (e2.status = $1 OR e2.status = $3)
                        AND e2.proof_validation_result IS NOT NULL
                        -- Invalid verdicts wait for fallback; valid verdicts wait for L1 publication.
                        AND (
                            (e2.proof_validation_result = FALSE AND e2.status = $3)
                            OR (
                                e2.proof_validation_result = TRUE
                                AND e2.l1_batch_number <= $2
                            )
                        )
                        AND e2.validated_proof_request_tx_hash IS NULL
                        -- The attempt count is a hard cap: a verdict the contract keeps
                        -- rejecting leaves the pool instead of reverting once per backoff
                        -- forever. `get_reached_max_attempts_amount` counts what is left.
                        AND e2.validation_tx_attempts < $4
                        AND (
                            e2.validation_tx_last_attempt_at IS NULL
                            OR e2.validation_tx_last_attempt_at < NOW() - $5::INTERVAL
                        )
                    ORDER BY e2.l1_batch_number ASC
                    LIMIT 1
                )
            RETURNING e.l1_batch_number, e.proof_validation_result;
            "#,
            EthProofManagerStatus::Proven.as_str(),
            latest_proven_batch,
            EthProofManagerStatus::Fallbacked.as_str(),
            max_attempts as i32,
            &retry_backoff,
        )
        .instrument("get_batch_to_validate")
        .fetch_optional(self.storage)
        .await?
        .map(|row| {
            (
                L1BatchNumber(row.l1_batch_number as u32),
                row.proof_validation_result,
            )
        });

        Ok(result.and_then(|(batch_number, validation_result)| {
            validation_result.map(|validation_result| (batch_number, validation_result))
        }))
    }

    /// Counts batches at or above the attempt limit whose verdict is still unreported.
    ///
    /// Rows already settled on chain are excluded even though they carry no transaction
    /// hash of ours, so the gauge keeps meaning "rewards currently locked".
    pub async fn get_reached_max_attempts_amount(&mut self, max_attempts: u64) -> DalResult<u64> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) FROM eth_proof_manager
            WHERE
                validation_tx_attempts >= $1
                AND validated_proof_request_tx_hash IS NULL
                AND status != $2
            "#,
            max_attempts as i32,
            EthProofManagerStatus::Validated.as_str(),
        )
        .instrument("get_reached_max_attempts_amount")
        .fetch_one(self.storage)
        .await?
        .count
        .unwrap_or(0) as u64;

        Ok(result)
    }

    pub async fn fallback_batches(
        &mut self,
        acknowledgment_timeout: Duration,
        proving_timeout: Duration,
        picking_timeout: Duration,
    ) -> DalResult<usize> {
        let acknowledgment_timeout = pg_interval_from_duration(acknowledgment_timeout);
        let proving_timeout = pg_interval_from_duration(proving_timeout);
        let picking_timeout = pg_interval_from_duration(picking_timeout);

        let mut transaction = self.storage.start_transaction().await?;

        // We move batches to fallback status if:
        // 1. The batch was sent but the proof request was not accepted after timeout
        // 2. The batch was acknowledged but the proof wasn't generated on time
        // 3. The batch was proven but the proof was invalid
        // 4. The batch submission outcome is uncertain and no acknowledgment was observed after timeout
        // 5. The batch was picked by prover network and is still preparing after timeout
        // 6. The batch was supposed to be picked by prover network, but it wasn't after timeout
        let batches: Vec<L1BatchNumber> = sqlx::query!(
            r#"
            UPDATE eth_proof_manager
            SET status = $1, updated_at = NOW()
            WHERE
                (status = $2 AND submit_proof_request_tx_sent_at < NOW() - $3::INTERVAL)
                OR (
                    status = $4
                    AND submit_proof_request_tx_sent_at < NOW() - $5::INTERVAL
                )
                OR (status = $6 AND proof_validation_result IS false)
                OR (
                    status = $7
                    AND submission_started_at < NOW() - $8::INTERVAL
                )
                OR (
                    (status = $9 OR status = $10)
                    AND updated_at < NOW() - $11::INTERVAL
                )
            RETURNING l1_batch_number
            "#,
            EthProofManagerStatus::Fallbacked.as_str(),
            EthProofManagerStatus::Sent.as_str(),
            &acknowledgment_timeout,
            EthProofManagerStatus::Acknowledged.as_str(),
            &proving_timeout,
            EthProofManagerStatus::Proven.as_str(),
            EthProofManagerStatus::Submitting.as_str(),
            &acknowledgment_timeout,
            EthProofManagerStatus::Preparing.as_str(),
            EthProofManagerStatus::Unpicked.as_str(),
            &picking_timeout,
        )
        .instrument("move_batches_to_fallback")
        .fetch_all(&mut transaction)
        .await?
        .into_iter()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32))
        .collect();
        let mut query_builder = QueryBuilder::new(
            "UPDATE proof_generation_details SET status='unpicked', proving_mode = 'prover_cluster', updated_at = NOW() WHERE (status='unpicked' AND updated_at < NOW() - "
        );

        query_builder.push_bind(picking_timeout);
        query_builder.push("::INTERVAL)");

        if !batches.is_empty() {
            query_builder.push(" OR (proving_mode = 'proving_network' AND l1_batch_number IN (");

            for (index, batch) in batches.iter().enumerate() {
                query_builder.push_bind(i64::from(batch.0));
                if index < batches.len() - 1 {
                    query_builder.push(", ");
                }
            }

            query_builder.push("))");
        }

        let result = query_builder
            .build()
            .instrument("move_batches_to_fallback")
            .execute(&mut transaction)
            .await?;

        tracing::info!("Moved {} batches to fallback", result.rows_affected());

        transaction.commit().await?;

        Ok(batches.len())
    }

    /// Requeues the batch to the cluster and rejects unreported verdicts on proven or fallbacked rows.
    pub async fn fallback_batch_after_l1_failure(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<()> {
        let mut transaction = self.storage.start_transaction().await?;

        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                proof_validation_result = FALSE,
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
                AND status IN ($1, $3)
                AND proof_validation_result IS NOT NULL
                AND validated_proof_request_tx_hash IS NULL
            "#,
            EthProofManagerStatus::Fallbacked.as_str(),
            i64::from(batch_number.0),
            EthProofManagerStatus::Proven.as_str(),
        )
        .instrument("fallback_batch_after_l1_failure#reject_reward")
        .with_arg("batch_number", &batch_number)
        .execute(&mut transaction)
        .await?;

        sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET status = 'unpicked', proving_mode = 'prover_cluster', updated_at = NOW()
            WHERE l1_batch_number = $1
            "#,
            i64::from(batch_number.0),
        )
        .instrument("fallback_batch_after_l1_failure#requeue")
        .with_arg("batch_number", &batch_number)
        .execute(&mut transaction)
        .await?;

        transaction.commit().await?;

        Ok(())
    }

    pub async fn fallback_batch(&mut self, batch_number: L1BatchNumber) -> DalResult<()> {
        let mut transaction = self.storage.start_transaction().await?;
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET status = $1, updated_at = NOW()
            WHERE l1_batch_number = $2
            "#,
            EthProofManagerStatus::Fallbacked.as_str(),
            i64::from(batch_number.0),
        )
        .instrument("fallback_certain_batch")
        .with_arg("batch_number", &batch_number)
        .execute(&mut transaction)
        .await?;

        // Replayed rejections leave existing cluster jobs untouched.
        sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET status = 'unpicked', proving_mode = 'prover_cluster', updated_at = NOW()
            WHERE l1_batch_number = $1 AND proving_mode = 'proving_network'
            "#,
            i64::from(batch_number.0),
        )
        .instrument("fallback_batch")
        .execute(&mut transaction)
        .await?;

        transaction.commit().await?;

        Ok(())
    }
}
