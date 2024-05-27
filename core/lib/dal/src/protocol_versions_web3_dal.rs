use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::api::ProtocolVersion;

use crate::{models::storage_protocol_version::StorageProtocolVersion, Core, CoreDal};

#[derive(Debug)]
pub struct ProtocolVersionsWeb3Dal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ProtocolVersionsWeb3Dal<'_, '_> {
    pub async fn get_protocol_version_by_id(
        &mut self,
        version_id: u16,
    ) -> DalResult<Option<ProtocolVersion>> {
        let storage_protocol_version = sqlx::query_as!(
            StorageProtocolVersion,
            r#"
            SELECT
                protocol_versions.id AS "minor!",
                protocol_versions.timestamp,
                protocol_versions.bootloader_code_hash,
                protocol_versions.default_account_code_hash,
                protocol_versions.upgrade_tx_hash,
                protocol_vk_patches.patch,
                protocol_vk_patches.recursion_scheduler_level_vk_hash,
                protocol_vk_patches.recursion_node_level_vk_hash,
                protocol_vk_patches.recursion_leaf_level_vk_hash,
                protocol_vk_patches.recursion_circuits_set_vks_hash
            FROM
                protocol_versions
                JOIN protocol_vk_patches ON protocol_vk_patches.minor = protocol_versions.id
            WHERE
                id = $1
            ORDER BY
                protocol_vk_patches.patch DESC
            LIMIT
                1
            "#,
            i32::from(version_id)
        )
        .instrument("get_protocol_version_by_id")
        .with_arg("version_id", &version_id)
        .fetch_optional(self.storage)
        .await?;

        Ok(storage_protocol_version.map(ProtocolVersion::from))
    }

    pub async fn get_latest_protocol_version(&mut self) -> DalResult<ProtocolVersion> {
        let latest_version = self
            .storage
            .protocol_versions_dal()
            .latest_semantic_version()
            .await?
            .unwrap();
        self.get_protocol_version_by_id(latest_version.minor as u16)
            .await
            .map(|v| v.unwrap())
    }
}
