use std::time::Duration;
use zksync_db_connection::{connection::Connection, error::DalResult, utils::pg_interval_from_duration};
use zksync_types::{L1BatchNumber, H256};

use crate::{Core, CoreDal};

#[derive(Debug)]
pub struct EthProofManagerDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

pub enum EthProofManagerStatus {
    Unpicked,
    Sent,
    Acknowledged,
    Proven,
    Fallbacked,
    Validated,
}

pub enum ProvingNetwork {
    None,
    Lagrange,
    Fermah,
}

impl EthProofManagerDal<'_, '_> {
    pub async fn insert_batch(&mut self, batch_number: L1BatchNumber, witness_inputs_url: &str) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO eth_proof_manager (l1_batch_number, status, created_at, witness_inputs_url, updated_at)
            VALUES ($1, $2, NOW(), $3, NOW())
            "#,
            batch_number,
            EthProofManagerStatus::Unpicked,
            witness_inputs_url
        )
        .instrument("insert_batch")
        .with_arg("batch_number", &batch_number)
        .with_arg("status", &EthProofManagerStatus::Unpicked)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn acknowledge_batch(&mut self, batch_number: L1BatchNumber, assigned_to: ProvingNetwork) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET status = $2, updated_at = NOW() WHERE l1_batch_number = $1
            "#,
            batch_number,
            EthProofManagerStatus::Acknowledged
        )
        .instrument("acknowledge_batch")
        .with_arg("batch_number", &batch_number)
        .with_arg("assigned_to", &assigned_to)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn mark_batch_as_sent(&mut self, batch_number: L1BatchNumber, tx_hash: H256) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET submit_proof_request_tx_hash = $2, updated_at = NOW(), status = $3 WHERE l1_batch_number = $1
            "#,
            batch_number,
            tx_hash.as_bytes(),
            EthProofManagerStatus::Sent
        )
        .instrument("mark_batch_as_sent")
        .with_arg("batch_number", &batch_number)
        .with_arg("tx_hash", &tx_hash)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn mark_batch_as_validated(&mut self, batch_number: L1BatchNumber, tx_hash: H256) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET validated_proof_request_tx_hash = $2, updated_at = NOW(), status = $3 WHERE l1_batch_number = $1
            "#,
            batch_number,
            tx_hash.as_bytes(),
            EthProofManagerStatus::Validated
        )
        .instrument("mark_batch_as_validated")
        .with_arg("batch_number", &batch_number)
        .with_arg("tx_hash", &tx_hash)
        .execute(self.storage)
        .await?;
    }

    pub async fn mark_batch_as_proven(&mut self, batch_number: L1BatchNumber, proof_blob_url: &str) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET proof_blob_url = $2, updated_at = NOW(), status = $3 WHERE l1_batch_number = $1
            "#,
            batch_number,
            proof_blob_url,
            EthProofManagerStatus::Proven
        )
        .instrument("mark_batch_as_proven")
        .with_arg("batch_number", &batch_number)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn update_status(&mut self, batch_number: L1BatchNumber, status: EthProofManagerStatus) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager SET status = $2, updated_at = NOW() WHERE l1_batch_number = $1
            "#,
            batch_number,
            status
        )
        .instrument("update_status")
        .with_arg("batch_number", &batch_number)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_batch_to_send(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let batch: Option<L1BatchNumber> = sqlx::query_as!(
            L1BatchNumber,
            r#"
            SELECT l1_batch_number, submit_proof_request_tx_hash FROM eth_proof_manager WHERE status = $1 ORDER BY created_at ASC LIMIT 1
            "#,
            EthProofManagerStatus::Unpicked
        )
        .fetch_optional(self.storage)
        .await?;

        Ok(batch)
    }

    pub async fn get_batch_to_validate(&mut self) -> DalResult<Option<(L1BatchNumber, String)>> {
        let row: Option<(L1BatchNumber, String)> = sqlx::query_as!(
            (L1BatchNumber, String),
            r#"
            SELECT l1_batch_number, proof_blob_url FROM eth_proof_manager WHERE status = $1 ORDER BY l1_batch_number ASC LIMIT 1
            "#,
            EthProofManagerStatus::Sent
        )
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|(batch_number, proof_blob_url)| (batch_number, proof_blob_url)))
    }

    pub async fn batches_to_fallback(&mut self, acknowledgment_timeout: Duration, proving_timeout: Duration, max_sending_attempts: u32) -> DalResult<usize> {
        let acknowledgment_timeout = pg_interval_from_duration(acknowledgment_timeout);
        let proving_timeout = pg_interval_from_duration(proving_timeout);

        let transaction = self.storage.start_transaction().await?;

        let batches: Vec<L1BatchNumber> = sqlx::query_as!(
            L1BatchNumber,
            r#"
                UPDATE eth_proof_manager
                SET status = $1, updated_at = NOW()
                WHERE (status = $2 AND submit_proof_request_tx_sent_at < NOW() - $5)
                OR (status = $3 AND validated_proof_request_tx_sent_at < NOW() - $6)
                OR (status = $4 AND submit_proof_request_attempts >= $7)
                RETURNING l1_batch_number
            "#,
            EthProofManagerStatus::Fallbacked,
            EthProofManagerStatus::Sent,
            EthProofManagerStatus::Acknowledged,
            EthProofManagerStatus::Unpicked,
            acknowledgment_timeout,
            proving_timeout,
            max_sending_attempts
        )
        .instrument("move_batches_to_fallback")
        .execute(transaction)
        .await?;

        for batch in batches {
        }        

        transaction.commit().await?;

        Ok(batches.len())
    }
}