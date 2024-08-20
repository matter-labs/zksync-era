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
        sqlx::query(&format!(
            "UPDATE prover_jobs_fri SET status = '{}' 
                WHERE l1_batch_number = {} 
                AND sequence_number = {} 
                AND aggregation_round = {}
                AND circuit_id = {}",
            status, batch_number.0, sequence_number, aggregation_round, circuit_id,
        ))
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
        sqlx::query(&format!(
            "
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
                ({}, {}, 'waiting_for_proofs', 2, NOW(), NOW())
            ON CONFLICT (l1_batch_number, circuit_id) DO
            UPDATE
            SET status = '{}'
            ",
            batch_number.0, circuit_id, status
        ))
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
        sqlx::query(&format!(
            "
            INSERT INTO
                node_aggregation_witness_jobs_fri (
                    l1_batch_number,
                    circuit_id,
                    status,
                    created_at,
                    updated_at
                )
            VALUES
                ({}, {}, 'waiting_for_proofs', NOW(), NOW())
            ON CONFLICT (l1_batch_number, circuit_id, depth) DO
            UPDATE
            SET status = '{}'
            ",
            batch_number.0, circuit_id, status,
        ))
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_rt_job(&mut self, status: WitnessJobStatus, batch_number: L1BatchNumber) {
        sqlx::query(&format!(
            "
            INSERT INTO
                recursion_tip_witness_jobs_fri (
                    l1_batch_number,
                    status,
                    number_of_final_node_jobs,
                    created_at,
                    updated_at
                )
            VALUES
                ({}, 'waiting_for_proofs',1, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET status = '{}'
            ",
            batch_number.0, status,
        ))
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_scheduler_job(
        &mut self,
        status: WitnessJobStatus,
        batch_number: L1BatchNumber,
    ) {
        sqlx::query(&format!(
            "
            INSERT INTO
                scheduler_witness_jobs_fri (
                    l1_batch_number,
                    scheduler_partial_input_blob_url,
                    status,
                    created_at,
                    updated_at
                )
            VALUES
                ({}, '', 'waiting_for_proofs', NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET status = '{}'
            ",
            batch_number.0, status,
        ))
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_compressor_job(
        &mut self,
        status: ProofCompressionJobStatus,
        batch_number: L1BatchNumber,
    ) {
        sqlx::query(&format!(
            "
            INSERT INTO
                proof_compression_jobs_fri (
                    l1_batch_number,
                    status,
                    created_at,
                    updated_at
                )
            VALUES
                ({}, '{}', NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET status = '{}'
            ",
            batch_number.0, status, status,
        ))
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
        sqlx::query(&format!(
            "UPDATE prover_jobs_fri 
                SET status = '{}', attempts = {} 
                WHERE l1_batch_number = {} 
                AND sequence_number = {} 
                AND aggregation_round = {}
                AND circuit_id = {}",
            status, attempts, batch_number.0, sequence_number, aggregation_round, circuit_id,
        ))
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
        sqlx::query(&format!(
            "UPDATE leaf_aggregation_witness_jobs_fri 
                SET status = '{}', attempts = {} 
                WHERE l1_batch_number = {}
                AND circuit_id = {}",
            status, attempts, batch_number.0, circuit_id,
        ))
        .execute(self.storage.conn())
        .await
        .unwrap();
    }
}
