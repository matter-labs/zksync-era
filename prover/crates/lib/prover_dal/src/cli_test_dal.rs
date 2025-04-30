use zksync_basic_types::{
    prover_dal::{ProofCompressionJobStatus, ProverJobStatus, WitnessJobStatus},
    L1BatchNumber,
};
use zksync_db_connection::connection::Connection;

use crate::Prover;

#[derive(Debug)]
pub struct CliTestDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Prover>,
}

impl CliTestDal<'_, '_> {
    pub async fn update_prover_job(
        &mut self,
        status: ProverJobStatus,
        circuit_id: u8,
        aggregation_round: i64,
        batch_number: L1BatchNumber,
        sequence_number: usize,
    ) {
        sqlx::query!(
            r#"
            UPDATE prover_jobs_fri
            SET
                status = $1
            WHERE
                l1_batch_number = $2
                AND sequence_number = $3
                AND aggregation_round = $4
                AND circuit_id = $5
            "#,
            status.to_string(),
            batch_number.0 as i64,
            sequence_number as i64,
            aggregation_round as i16,
            circuit_id as i64,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_lwg_job(
        &mut self,
        status: WitnessJobStatus,
        batch_number: L1BatchNumber,
        circuit_id: u8,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            leaf_aggregation_witness_jobs_fri (
                l1_batch_number,
                circuit_id,
                status,
                number_of_basic_circuits,
                created_at,
                updated_at
            )
            VALUES
            ($1, $2, 'waiting_for_proofs', 2, NOW(), NOW())
            ON CONFLICT (l1_batch_number, chain_id, circuit_id) DO
            UPDATE
            SET
            status = $3
            "#,
            batch_number.0 as i64,
            circuit_id as i16,
            status.to_string()
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_nwg_job(
        &mut self,
        status: WitnessJobStatus,
        batch_number: L1BatchNumber,
        circuit_id: u8,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            node_aggregation_witness_jobs_fri (
                l1_batch_number, circuit_id, status, created_at, updated_at
            )
            VALUES
            ($1, $2, 'waiting_for_proofs', NOW(), NOW())
            ON CONFLICT (l1_batch_number, chain_id, circuit_id, depth) DO
            UPDATE
            SET
            status = $3
            "#,
            batch_number.0 as i64,
            circuit_id as i16,
            status.to_string(),
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_rt_job(&mut self, status: WitnessJobStatus, batch_number: L1BatchNumber) {
        sqlx::query!(
            r#"
            INSERT INTO
            recursion_tip_witness_jobs_fri (
                l1_batch_number, status, number_of_final_node_jobs, created_at, updated_at
            )
            VALUES
            ($1, 'waiting_for_proofs', 1, NOW(), NOW())
            ON CONFLICT (l1_batch_number, chain_id) DO
            UPDATE
            SET
            status = $2
            "#,
            batch_number.0 as i64,
            status.to_string(),
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_scheduler_job(
        &mut self,
        status: WitnessJobStatus,
        batch_number: L1BatchNumber,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            scheduler_witness_jobs_fri (
                l1_batch_number,
                scheduler_partial_input_blob_url,
                status,
                created_at,
                updated_at
            )
            VALUES
            ($1, '', 'waiting_for_proofs', NOW(), NOW())
            ON CONFLICT (l1_batch_number, chain_id) DO
            UPDATE
            SET
            status = $2
            "#,
            batch_number.0 as i64,
            status.to_string(),
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_compressor_job(
        &mut self,
        status: ProofCompressionJobStatus,
        batch_number: L1BatchNumber,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            proof_compression_jobs_fri (l1_batch_number, status, created_at, updated_at)
            VALUES
            ($1, $2, NOW(), NOW())
            ON CONFLICT (l1_batch_number, chain_id) DO
            UPDATE
            SET
            status = $2
            "#,
            batch_number.0 as i64,
            status.to_string(),
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn update_attempts_prover_job(
        &mut self,
        status: ProverJobStatus,
        attempts: u8,
        circuit_id: u8,
        aggregation_round: i64,
        batch_number: L1BatchNumber,
        sequence_number: usize,
    ) {
        sqlx::query!(
            r#"
            UPDATE prover_jobs_fri
            SET
                status = $1,
                attempts = $2
            WHERE
                l1_batch_number = $3
                AND sequence_number = $4
                AND aggregation_round = $5
                AND circuit_id = $6
            "#,
            status.to_string(),
            attempts as i64,
            batch_number.0 as i64,
            sequence_number as i64,
            aggregation_round as i64,
            circuit_id as i16,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn update_attempts_lwg(
        &mut self,
        status: ProverJobStatus,
        attempts: u8,
        circuit_id: u8,
        batch_number: L1BatchNumber,
    ) {
        sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
            SET
                status = $1,
                attempts = $2
            WHERE
                l1_batch_number = $3
                AND circuit_id = $4
            "#,
            status.to_string(),
            attempts as i64,
            batch_number.0 as i64,
            circuit_id as i16,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }
}
