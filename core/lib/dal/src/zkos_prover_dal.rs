use std::time::Duration;
use zksync_db_connection::connection::Connection;
use zksync_db_connection::error::DalResult;
use zksync_types::L2BlockNumber;
use crate::Core;
use zksync_db_connection::{
    instrument::{CopyStatement, InstrumentExt},
    utils::pg_interval_from_duration,
};

#[derive(Debug)]
pub struct ZkosProverDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl ZkosProverDal<'_, '_> {
    /// returns 0 if no inputs are found
    pub async fn last_block_with_generated_input(
        &mut self,
    ) -> DalResult<L2BlockNumber> {
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(l2_block_number) AS "number"
            FROM
                zkos_proofs
            "#
        )
            .instrument("last_block_with_generated_input")
            .report_latency()
            .fetch_one(self.storage)
            .await?;
        let number = row.number.unwrap_or(0) as u32;
        Ok(L2BlockNumber(number))
    }

    pub async fn insert_prover_input(
        &mut self,
        l2block_number: L2BlockNumber,
        prover_input: Vec<u32>,
        time_taken: Duration,
    ) -> DalResult<()> {
        let db_prover_input: Vec<u8> = prover_input
            .into_iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();

        sqlx::query!(
            r#"
            INSERT INTO
                zkos_proofs (
                    l2_block_number,
                    prover_input,
                    input_gen_time_taken,
                    created_at,
                    updated_at
                )
            VALUES
                ($1, $2, $3::INTERVAL, NOW(), NOW())
            "#,
            i64::from(l2block_number.0),
            db_prover_input,
            pg_interval_from_duration(time_taken)
        )
            .instrument("insert_prover_input")
            .report_latency()
            .execute(self.storage)
            .await?;
        Ok(())
    }

    /// Pick the next block for an FRI proof
    pub async fn pick_next_fri_proof(
        &mut self,
        timeout: Duration,
        picked_by: &str,
    ) -> DalResult<Option<(L2BlockNumber, Vec<u8>)>> {
        let maybe_row = sqlx::query!(
            r#"
            WITH next_fri AS (
                SELECT l2_block_number
                FROM zkos_proofs
                WHERE prover_input IS NOT NULL
                  AND (
                      fri_proof_picked_at IS NULL
                      OR fri_proof_picked_at < NOW() - $1::INTERVAL
                  )
                ORDER BY l2_block_number ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE zkos_proofs
            SET
                fri_proof_picked_at     = NOW(),
                fri_proof_picked_by     = $2,
                fri_proof_attempts      = COALESCE(fri_proof_attempts, 0) + 1,
                updated_at              = NOW()
            FROM next_fri
            WHERE zkos_proofs.l2_block_number = next_fri.l2_block_number
            RETURNING
                zkos_proofs.l2_block_number AS "l2_block_number!",
                zkos_proofs.prover_input     AS "prover_input!"
            "#,
            pg_interval_from_duration(timeout),
            picked_by,
        )
            .instrument("pick_next_fri_proof")
            .report_latency()
            .fetch_optional(self.storage)
            .await?;

        Ok(maybe_row.map(|row| {
            (L2BlockNumber(row.l2_block_number as u32), row.prover_input)
        }))
    }

    /// Save a completed FRI proof
    pub async fn save_fri_proof(
        &mut self,
        block_number: L2BlockNumber,
        proof: Vec<u8>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE zkos_proofs
            SET
                fri_prove_time_taken = NOW() - fri_proof_picked_at,
                fri_proof            = $2,
                updated_at           = NOW()
            WHERE l2_block_number = $1
            "#,
            i64::from(block_number.0),
            proof,
        )
            .instrument("save_fri_proof")
            .report_latency()
            .execute(self.storage)
            .await?;

        Ok(())
    }

    /// Pick the next block for a SNARK proof
    pub async fn pick_next_snark_proof(
        &mut self,
        timeout: Duration,
        picked_by: &str,
    ) -> DalResult<Option<(L2BlockNumber, Vec<u8>)>> {
        let maybe_row = sqlx::query!(
            r#"
            WITH next_snark AS (
                SELECT l2_block_number
                FROM zkos_proofs
                WHERE fri_proof IS NOT NULL
                  AND (
                      snark_proof_picked_at IS NULL
                      OR snark_proof_picked_at < NOW() - $1::INTERVAL
                  )
                ORDER BY l2_block_number ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE zkos_proofs
            SET
                snark_proof_picked_at    = NOW(),
                snark_proof_picked_by    = $2,
                snark_proof_attempts     = snark_proof_attempts + 1,
                updated_at               = NOW()
            FROM next_snark
            WHERE zkos_proofs.l2_block_number = next_snark.l2_block_number
            RETURNING
                zkos_proofs.l2_block_number AS "l2_block_number!",
                zkos_proofs.fri_proof       AS "fri_proof!"
            "#,
            pg_interval_from_duration(timeout),
            picked_by,
        )
            .instrument("pick_next_snark_proof")
            .report_latency()
            .fetch_optional(self.storage)
            .await?;

        Ok(maybe_row.map(|row| {
            (L2BlockNumber(row.l2_block_number as u32), row.fri_proof)
        }))
    }

    /// Save a completed SNARK proof
    pub async fn save_snark_proof(
        &mut self,
        block_number: L2BlockNumber,
        proof: Vec<u8>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE zkos_proofs
            SET
                snark_prove_time_taken = NOW() - snark_proof_picked_at,
                snark_proof            = $2,
                updated_at             = NOW()
            WHERE l2_block_number = $1
            "#,
            i64::from(block_number.0),
            proof,
        )
            .instrument("save_snark_proof")
            .report_latency()
            .execute(self.storage)
            .await?;

        Ok(())
    }
    pub async fn list_available_proofs(
        &mut self,
    ) -> DalResult<Vec<(L2BlockNumber, Vec<String>)>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                l2_block_number,
                fri_proof IS NOT NULL   AS "has_fri!",
                snark_proof IS NOT NULL AS "has_snark!"
            FROM zkos_proofs
            ORDER BY l2_block_number
            "#
        )
            .instrument("list_available_proofs")
            .report_latency()
            .fetch_all(self.storage)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let mut proofs = Vec::new();
                if row.has_fri {
                    proofs.push("FRI".to_string());
                }
                if row.has_snark {
                    proofs.push("SNARK".to_string());
                }
                (L2BlockNumber(row.l2_block_number as u32), proofs)
            })
            .collect())
    }

    /// Retrieve the stored FRI proof for a given block, if present.
    pub async fn get_fri_proof(
        &mut self,
        block_number: L2BlockNumber,
    ) -> DalResult<Option<Vec<u8>>> {
        let row = sqlx::query!(
            r#"
            SELECT fri_proof
            FROM zkos_proofs
            WHERE l2_block_number = $1
            "#,
            i64::from(block_number.0),
        )
            .instrument("get_fri_proof")
            .report_latency()
            .fetch_optional(self.storage)
            .await?;

        Ok(row.and_then(|r| r.fri_proof))
    }

    /// Retrieve the stored SNARK proof for a given block, if present.
    pub async fn get_snark_proof(
        &mut self,
        block_number: L2BlockNumber,
    ) -> DalResult<Option<Vec<u8>>> {
        let row = sqlx::query!(
            r#"
            SELECT snark_proof
            FROM zkos_proofs
            WHERE l2_block_number = $1
            "#,
            i64::from(block_number.0),
        )
            .instrument("get_snark_proof")
            .report_latency()
            .fetch_optional(self.storage)
            .await?;

        Ok(row.and_then(|r| r.snark_proof))
    }

}
