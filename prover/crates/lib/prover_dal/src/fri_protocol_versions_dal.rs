use zksync_basic_types::{
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    H256,
};
use zksync_db_connection::connection::Connection;

use crate::Prover;

#[derive(Debug)]
pub struct FriProtocolVersionsDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Prover>,
}

impl FriProtocolVersionsDal<'_, '_> {
    pub async fn save_prover_protocol_version(
        &mut self,
        id: ProtocolSemanticVersion,
        l1_verifier_config: L1VerifierConfig,
    ) {
        // `recursion_scheduler_level_vk_hash` column actually stores `snark_wrapper_vk_hash`
        sqlx::query!(
            r#"
            INSERT INTO
                prover_fri_protocol_versions (id, recursion_scheduler_level_vk_hash, created_at, protocol_version_patch)
            VALUES
                ($1, $2, NOW(), $3)
            ON CONFLICT (id, protocol_version_patch) DO NOTHING
            "#,
            id.minor as i32,
            l1_verifier_config
                .snark_wrapper_vk_hash
                .as_bytes(),
            id.patch.0 as i32
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn vk_commitments_for(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
    ) -> Option<L1VerifierConfig> {
        sqlx::query!(
            r#"
            SELECT
                recursion_scheduler_level_vk_hash AS snark_wrapper_vk_hash
            FROM
                prover_fri_protocol_versions
            WHERE
                id = $1
                AND protocol_version_patch = $2
            "#,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1VerifierConfig {
            snark_wrapper_vk_hash: H256::from_slice(&row.snark_wrapper_vk_hash),
        })
    }

    pub async fn get_l1_verifier_config(&mut self) -> Result<L1VerifierConfig, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT
                recursion_scheduler_level_vk_hash AS snark_wrapper_vk_hash
            FROM
                prover_fri_protocol_versions
            ORDER BY
                id DESC
            LIMIT
                1
            "#,
        )
        .fetch_one(self.storage.conn())
        .await?;

        Ok(L1VerifierConfig {
            snark_wrapper_vk_hash: H256::from_slice(&result.snark_wrapper_vk_hash),
        })
    }

    pub async fn delete(&mut self) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM prover_fri_protocol_versions
            "#
        )
        .execute(self.storage.conn())
        .await
    }
}
