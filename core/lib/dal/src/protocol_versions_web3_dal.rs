use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::api::{ProtocolVersion, ProtocolVersionInfo};

use crate::{models::storage_protocol_version::StorageApiProtocolVersion, Core, CoreDal};

#[derive(Debug)]
pub struct ProtocolVersionsWeb3Dal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ProtocolVersionsWeb3Dal<'_, '_> {
    #[deprecated]
    pub async fn get_protocol_version_by_id(
        &mut self,
        version_id: u16,
    ) -> DalResult<Option<ProtocolVersion>> {
        let storage_protocol_version = sqlx::query_as!(
            StorageApiProtocolVersion,
            r#"
            SELECT
                id AS "minor!",
                timestamp,
                bootloader_code_hash,
                default_account_code_hash,
                evm_emulator_code_hash,
                upgrade_tx_hash
            FROM
                protocol_versions
            WHERE
                id = $1
            "#,
            i32::from(version_id)
        )
        .instrument("get_protocol_version_by_id")
        .with_arg("version_id", &version_id)
        .fetch_optional(self.storage)
        .await?;

        Ok(storage_protocol_version.map(ProtocolVersion::from))
    }

    #[deprecated]
    #[allow(deprecated)]
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

    pub async fn get_protocol_version_info_by_id(
        &mut self,
        version_id: u16,
    ) -> DalResult<Option<ProtocolVersionInfo>> {
        let storage_protocol_version = sqlx::query_as!(
            StorageApiProtocolVersion,
            r#"
            SELECT
                id AS "minor!",
                timestamp,
                bootloader_code_hash,
                default_account_code_hash,
                evm_emulator_code_hash,
                upgrade_tx_hash
            FROM
                protocol_versions
            WHERE
                id = $1
            "#,
            i32::from(version_id)
        )
        .instrument("get_protocol_version_by_id")
        .with_arg("version_id", &version_id)
        .fetch_optional(self.storage)
        .await?;

        Ok(storage_protocol_version.map(ProtocolVersionInfo::from))
    }
}
