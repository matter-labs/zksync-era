use std::str::FromStr;

use zksync_db_connection::{
    connection::Connection,
    error::{DalError, DalResult},
    instrument::InstrumentExt,
};
use zksync_types::{L1BatchNumber, H256};

use crate::Core;

#[derive(Debug)]
pub struct EthProofManagerDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug)]
pub enum ProofStatus {
    ReadyToBeProven,
    RequestSentToL1,
    Acknowledged,
    ReceivedProof,
    ValidationResultSentToL1,
}

#[derive(Debug, PartialEq)]
pub enum ValidationStatus {
    Failed,
    Successful,
}

impl FromStr for ValidationStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "failed" => ValidationStatus::Failed,
            "successful" => ValidationStatus::Successful,
            _ => anyhow::bail!("Invalid validation status: {}", s),
        })
    }
}

impl From<ValidationStatus> for &str {
    fn from(status: ValidationStatus) -> Self {
        match status {
            ValidationStatus::Failed => "failed",
            ValidationStatus::Successful => "successful",
        }
    }
}

impl FromStr for ProofStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "ready_to_be_proven" => ProofStatus::ReadyToBeProven,
            "request_sent_to_l1" => ProofStatus::RequestSentToL1,
            "acknowledged" => ProofStatus::Acknowledged,
            "received_proof" => ProofStatus::ReceivedProof,
            "validation_result_sent_to_l1" => ProofStatus::ValidationResultSentToL1,
            _ => anyhow::bail!("Invalid proof status: {}", s),
        })
    }
}

impl From<ProofStatus> for &str {
    fn from(status: ProofStatus) -> Self {
        match status {
            ProofStatus::ReadyToBeProven => "ready_to_be_proven",
            ProofStatus::RequestSentToL1 => "request_sent_to_l1",
            ProofStatus::Acknowledged => "acknowledged",
            ProofStatus::ReceivedProof => "received_proof",
            ProofStatus::ValidationResultSentToL1 => "validation_result_sent_to_l1",
        }
    }
}

impl EthProofManagerDal<'_, '_> {
    pub async fn insert_proof_request(
        &mut self,
        l1_batch_number: u64,
        proof_gen_data_blob_url: String,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            l1_batch_number, proof_gen_data_blob_url, status
            ) VALUES ($1, $2, $3)
            "#,
            l1_batch_number.0 as i64,
            &proof_gen_data_blob_url,
            ProofStatus::ReadyToBeProven.into()
        )
        .instrument("insert_proof_request")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_ready_to_be_proven_batches(
        &mut self,
    ) -> DalResult<Option<(L1BatchNumber, String)>> {
        let row = sqlx::query!(
            r#"
            SELECT l1_batch_number, proof_gen_data_blob_url
            FROM eth_proof_manager
            WHERE status = $1
            "#,
            ProofStatus::ReadyToSend.into()
        )
        .instrument("get_ready_to_be_proven_batches")
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| {
            (
                L1BatchNumber(row.l1_batch_number as u32),
                row.proof_gen_data_blob_url,
            )
        }))
    }

    pub async fn get_validated_not_sent_batch(
        &mut self,
    ) -> anyhow::Result<Option<(L1BatchNumber, ValidationStatus)>> {
        let row = sqlx::query!(
            r#"
            FROM eth_proof_manager
            WHERE status = $1 AND validation_status IS NOT NULL
            "#,
            ProofStatus::ReceivedProof.into()
        )
        .instrument("get_validated_not_sent_batch")
        .fetch_optional(self.storage)
        .await?;

        if let Some(row) = row {
            let validation_status = ValidationStatus::from_str(&row.validation_status)
                .map_err(|e| anyhow::anyhow!("Invalid validation status: {}", e))?;
            if validation_status == ValidationStatus::Successful {
                return Ok(Some((
                    L1BatchNumber(row.l1_batch_number),
                    validation_status,
                )));
            }
        }

        Ok(None)
    }

    pub async fn save_validation_result(
        &mut self,
        l1_batch_number: L1BatchNumber,
        validation_succeeded: bool,
    ) -> DalResult<()> {
        let validation_status = if validation_succeeded {
            ValidationStatus::Successful
        } else {
            ValidationStatus::Failed
        };

        sqlx::query!(
            r#"
            UPDATE eth_proof_manager
            SET validation_status = $1
            WHERE l1_batch_number = $2
            "#,
            validation_status.into(),
            l1_batch_number.0 as i64
        )
        .instrument("save_validation_result")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn mark_proof_request_as_sent_to_l1(
        &mut self,
        l1_batch_number: L1BatchNumber,
        transaction_hash: H256,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager
            SET status = $1, transaction_hash = $2, request_sent_at = NOW()
            WHERE l1_batch_number = $3
            "#,
            ProofStatus::RequestSentToL1.into(),
            transaction_hash.as_bytes(),
            l1_batch_number.0 as i64
        )
        .instrument("mark_proof_request_as_sent_to_l1")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn mark_proof_request_as_acknowledged(
        &mut self,
        l1_batch_number: L1BatchNumber,
        proving_network: ProvingNetwork,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager
            SET status = $1, assigned_to = $2, acknowledged_at = NOW()
            WHERE l1_batch_number = $3
            "#,
            ProofStatus::Acknowledged.into(),
            proving_network.into(),
            l1_batch_number.0 as i64
        )
        .instrument("mark_proof_request_as_acknowledged")
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn mark_proof_request_validation_result_as_sent_to_l1(
        &mut self,
        l1_batch_number: L1BatchNumber,
        transaction_hash: H256,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE eth_proof_manager
            SET status = $1, transaction_hash = $2, validation_result_sent_at = NOW()
            WHERE l1_batch_number = $3
            "#,
            ProofStatus::ValidationResultSentToL1.into(),
            transaction_hash.as_bytes(),
            l1_batch_number.0 as i64
        )
        .instrument("mark_proof_request_validation_result_as_sent_to_l1")
        .execute(self.storage)
        .await?;

        Ok(())
    }
}
