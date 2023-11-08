use std::convert::TryFrom;
use zksync_types::{
    protocol_version::{L1VerifierConfig, ProtocolVersion},
    ProtocolVersionId,
};

use crate::ProverStorageProcessor;

#[derive(Debug)]
pub struct ProverProtocolVersionsDal<'a, 'c> {
    pub storage: &'a mut ProverStorageProcessor<'c>,
}

impl ProverProtocolVersionsDal<'_, '_> {
    pub async fn save_prover_protocol_version(&mut self, version: ProtocolVersion) {
        sqlx::query!(
                "INSERT INTO prover_protocol_versions
                    (id, timestamp, recursion_scheduler_level_vk_hash, recursion_node_level_vk_hash,
                        recursion_leaf_level_vk_hash, recursion_circuits_set_vks_hash, verifier_address, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, now())
                ",
                version.id as i32,
                version.timestamp as i64,
                version.l1_verifier_config.recursion_scheduler_level_vk_hash.as_bytes(),
                version.l1_verifier_config.params.recursion_node_level_vk_hash.as_bytes(),
                version.l1_verifier_config.params.recursion_leaf_level_vk_hash.as_bytes(),
                version.l1_verifier_config.params.recursion_circuits_set_vks_hash.as_bytes(),
                version.verifier_address.as_bytes(),
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
    }

    pub async fn protocol_version_for(
        &mut self,
        vk_commitments: &L1VerifierConfig,
    ) -> Vec<ProtocolVersionId> {
        sqlx::query!(
            r#"
                SELECT id
                FROM prover_protocol_versions
                WHERE recursion_circuits_set_vks_hash = $1
                AND recursion_leaf_level_vk_hash = $2
                AND recursion_node_level_vk_hash = $3
                AND recursion_scheduler_level_vk_hash = $4
               "#,
            vk_commitments
                .params
                .recursion_circuits_set_vks_hash
                .as_bytes(),
            vk_commitments
                .params
                .recursion_leaf_level_vk_hash
                .as_bytes(),
            vk_commitments
                .params
                .recursion_node_level_vk_hash
                .as_bytes(),
            vk_commitments.recursion_scheduler_level_vk_hash.as_bytes(),
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| ProtocolVersionId::try_from(row.id as u16).unwrap())
        .collect()
    }

    pub async fn prover_protocol_version_exists(&mut self, id: ProtocolVersionId) -> bool {
        sqlx::query!(
            "SELECT COUNT(*) as \"count!\" FROM prover_protocol_versions \
            WHERE id = $1",
            id as i32
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .count
            > 0
    }
}
