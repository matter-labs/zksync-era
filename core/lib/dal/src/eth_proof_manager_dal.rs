use std::time::Duration;

use sqlx::QueryBuilder;
use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::InstrumentExt,
    utils::pg_interval_from_duration,
};
use zksync_types::{L1BatchNumber, H256};

use crate::Core;

#[derive(Debug)]
pub struct EthProofManagerDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, Clone)]
pub enum EthProofManagerStatus {
    Unpicked,
    Sent,
    Acknowledged,
    Proven,
    Fallbacked,
    Validated,
}

impl EthProofManagerStatus {
    pub fn as_str(&self) -> &str {
        match self {
            EthProofManagerStatus::Unpicked => "unpicked",
            EthProofManagerStatus::Sent => "sent",
            EthProofManagerStatus::Acknowledged => "acknowledged",
            EthProofManagerStatus::Proven => "proven",
            EthProofManagerStatus::Fallbacked => "fallbacked",
            EthProofManagerStatus::Validated => "validated",
        }
    }

    pub fn from_str(status: &str) -> EthProofManagerStatus {
        match status {
            "unpicked" => EthProofManagerStatus::Unpicked,
            "sent" => EthProofManagerStatus::Sent,
            "acknowledged" => EthProofManagerStatus::Acknowledged,
            "proven" => EthProofManagerStatus::Proven,
            "fallbacked" => EthProofManagerStatus::Fallbacked,
            "validated" => EthProofManagerStatus::Validated,
            _ => panic!("Invalid status: {}", status),
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

    pub fn from_str(status: &str) -> ProvingNetwork {
        match status {
            "none" => ProvingNetwork::None,
            "lagrange" => ProvingNetwork::Lagrange,
            "fermah" => ProvingNetwork::Fermah,
            _ => panic!("Invalid proving network: {}", status),
        }
    }
}

impl EthProofManagerDal<'_, '_> {
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
            batch_number.0 as i64,
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

    pub async fn acknowledge_batch(
        &mut self,
        batch_number: L1BatchNumber,
        assigned_to: ProvingNetwork,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET status = $2, updated_at = NOW(), assigned_to = $3
            WHERE l1_batch_number = $1
            "#,
            batch_number.0 as i64,
            EthProofManagerStatus::Acknowledged.as_str(),
            assigned_to.as_str()
        )
        .instrument("acknowledge_batch")
        .with_arg("batch_number", &batch_number)
        .with_arg("assigned_to", &assigned_to)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn mark_batch_as_sent(
        &mut self,
        batch_number: L1BatchNumber,
        tx_hash: H256,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                submit_proof_request_tx_hash = $2,
                submit_proof_request_tx_sent_at = NOW(),
                updated_at = NOW(),
                status = $3
            WHERE l1_batch_number = $1
            "#,
            batch_number.0 as i64,
            tx_hash.as_bytes(),
            EthProofManagerStatus::Sent.as_str()
        )
        .instrument("mark_batch_as_sent")
        .with_arg("batch_number", &batch_number)
        .with_arg("tx_hash", &tx_hash)
        .execute(self.storage)
        .await?;

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
            batch_number.0 as i64,
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

    pub async fn mark_batch_as_proven(
        &mut self,
        batch_number: L1BatchNumber,
        proof_validation_result: bool,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET
                proof_validation_result = $2,
                updated_at = NOW(),
                status = $3
            WHERE l1_batch_number = $1
            "#,
            batch_number.0 as i64,
            proof_validation_result,
            EthProofManagerStatus::Proven.as_str()
        )
        .instrument("mark_batch_as_proven")
        .with_arg("batch_number", &batch_number)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn update_status(
        &mut self,
        batch_number: L1BatchNumber,
        status: EthProofManagerStatus,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET status = $2, updated_at = NOW()
            WHERE l1_batch_number = $1
            "#,
            batch_number.0 as i64,
            status.as_str()
        )
        .instrument("update_status")
        .with_arg("batch_number", &batch_number)
        .with_arg("status", &status.as_str())
        .execute(self.storage)
        .await?;

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

    pub async fn get_batch_to_send_validation_result(
        &mut self,
    ) -> anyhow::Result<Option<(L1BatchNumber, bool)>> {
        let result: Option<(L1BatchNumber, Option<bool>)> = sqlx::query!(
            r#"
            SELECT l1_batch_number, proof_validation_result
            FROM eth_proof_manager
            WHERE status = $1
            ORDER BY l1_batch_number ASC
            LIMIT 1
            "#,
            EthProofManagerStatus::Proven.as_str()
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

        match result {
            Some((batch_number, Some(validation_result))) => {
                Ok(Some((batch_number, validation_result)))
            }
            _ => Err(anyhow::anyhow!("No batch to send validation result")),
        }
    }

    pub async fn fallback_batches(
        &mut self,
        acknowledgment_timeout: Duration,
        proving_timeout: Duration,
        picking_timeout: Duration,
    ) -> DalResult<usize> {
        let acknowledgment_timeout = pg_interval_from_duration(acknowledgment_timeout);
        let proving_timeout = pg_interval_from_duration(proving_timeout);

        let mut transaction = self.storage.start_transaction().await?;

        // We move batches to fallback status if:
        // 1. The batch was sent but the proof request was not accepted after timeout
        // 2. The batch was acknowledged but the proof wasn't generated on time
        // 3. The batch was not sent after max attempts
        // 4. The batch was proven but the proof was invalid
        let batches: Vec<L1BatchNumber> = sqlx::query!(
            r#"
            UPDATE eth_proof_manager
            SET status = $1, updated_at = NOW()
            WHERE
                (status = $2 AND submit_proof_request_tx_sent_at < NOW() - $3::INTERVAL)
                OR (
                    status = $4
                    AND validated_proof_request_tx_sent_at < NOW() - $5::INTERVAL
                )
                OR (status = $6 AND proof_validation_result IS false)
            RETURNING l1_batch_number
            "#,
            EthProofManagerStatus::Fallbacked.as_str(),
            EthProofManagerStatus::Sent.as_str(),
            &acknowledgment_timeout,
            EthProofManagerStatus::Acknowledged.as_str(),
            &proving_timeout,
            EthProofManagerStatus::Validated.as_str()
        )
        .instrument("move_batches_to_fallback")
        .fetch_all(&mut transaction)
        .await?
        .into_iter()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32))
        .collect();

        if !batches.is_empty() {
            let mut query_builder = QueryBuilder::new(
                "UPDATE proof_generation_details SET status = 'fallbacked', updated_at = NOW() WHERE (status='unpicked' AND updated_at < NOW() - $1::INTERVAL) OR l1_batch_number IN ("
            );

            let timeout = pg_interval_from_duration(picking_timeout);
            query_builder.push_bind(&timeout);

            for (index, batch) in batches.iter().enumerate() {
                query_builder.push_bind(batch.0 as i64);
                if index < batches.len() - 1 {
                    query_builder.push(", ");
                }
            }

            query_builder.push(")");

            let result = query_builder
                .build()
                .instrument("move_batches_to_fallback")
                .execute(&mut transaction)
                .await?;

            tracing::info!("Moved {} batches to fallback", result.rows_affected());
        }

        transaction.commit().await?;

        Ok(batches.len())
    }

    pub async fn fallback_certain_batch(&mut self, batch_number: L1BatchNumber) -> DalResult<()> {
        let mut transaction = self.storage.start_transaction().await?;
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET status = $1, updated_at = NOW()
            WHERE l1_batch_number = $2
            "#,
            EthProofManagerStatus::Fallbacked.as_str(),
            batch_number.0 as i64
        )
        .instrument("fallback_certain_batch")
        .with_arg("batch_number", &batch_number)
        .execute(&mut transaction)
        .await?;

        sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET status = 'fallbacked', updated_at = NOW()
            WHERE l1_batch_number = $1
            "#,
            batch_number.0 as i64
        )
        .instrument("fallback_certain_batch")
        .execute(&mut transaction)
        .await?;

        transaction.commit().await?;

        Ok(())
    }
}
