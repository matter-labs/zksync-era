use zksync_basic_types::{
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    H256,
};
use zksync_db_connection::{connection::Connection, error::DalError, instrument::InstrumentExt};

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
    ) -> Result<(), DalError> {
        sqlx::query!(
            r#"
            INSERT INTO
            prover_fri_protocol_versions (
                id,
                snark_wrapper_vk_hash,
                fflonk_snark_wrapper_vk_hash,
                created_at,
                protocol_version_patch
            )
            VALUES
            ($1, $2, $3, NOW(), $4)
            ON CONFLICT (id, protocol_version_patch) DO NOTHING
            "#,
            id.minor as i32,
            l1_verifier_config.snark_wrapper_vk_hash.as_bytes(),
            l1_verifier_config
                .fflonk_snark_wrapper_vk_hash
                .as_ref()
                .map(|x| x.as_bytes()),
            id.patch.0 as i32
        )
        .instrument("save_prover_protocol_version")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn vk_commitments_for(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
    ) -> Option<L1VerifierConfig> {
        sqlx::query!(
            r#"
            SELECT
                snark_wrapper_vk_hash,
                fflonk_snark_wrapper_vk_hash
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
            fflonk_snark_wrapper_vk_hash: row
                .fflonk_snark_wrapper_vk_hash
                .as_ref()
                .map(|x| H256::from_slice(x)),
        })
    }

    pub async fn get_l1_verifier_config(&mut self) -> Result<L1VerifierConfig, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT
                snark_wrapper_vk_hash,
                fflonk_snark_wrapper_vk_hash
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
            fflonk_snark_wrapper_vk_hash: result
                .fflonk_snark_wrapper_vk_hash
                .as_ref()
                .map(|x| H256::from_slice(x)),
        })
    }
}
