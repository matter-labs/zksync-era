use std::convert::TryFrom;

use zksync_basic_types::protocol_version::{L1VerifierConfig, ProtocolVersionId};
use zksync_db_connection::connection::Connection;

use crate::Prover;

#[derive(Debug)]
pub struct FriProtocolVersionsDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Prover>,
}

impl FriProtocolVersionsDal<'_, '_> {
    pub async fn save_prover_protocol_version(
        &mut self,
        id: ProtocolVersionId,
        l1_verifier_config: L1VerifierConfig,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
                prover_fri_protocol_versions (
                    id,
                    recursion_scheduler_level_vk_hash,
                    recursion_node_level_vk_hash,
                    recursion_leaf_level_vk_hash,
                    recursion_circuits_set_vks_hash,
                    created_at
                )
            VALUES
                ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (id) DO NOTHING
            "#,
            id as i32,
            l1_verifier_config
                .recursion_scheduler_level_vk_hash
                .as_bytes(),
            l1_verifier_config
                .params
                .recursion_node_level_vk_hash
                .as_bytes(),
            l1_verifier_config
                .params
                .recursion_leaf_level_vk_hash
                .as_bytes(),
            l1_verifier_config
                .params
                .recursion_circuits_set_vks_hash
                .as_bytes(),
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
            SELECT
                id
            FROM
                prover_fri_protocol_versions
            WHERE
                recursion_circuits_set_vks_hash = $1
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
}
