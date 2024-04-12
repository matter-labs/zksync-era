use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::api::ProtocolVersion;

use crate::{models::storage_protocol_version::StorageProtocolVersion, Core};

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
                *
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

    pub async fn get_latest_protocol_version(&mut self) -> DalResult<ProtocolVersion> {
        let storage_protocol_version = sqlx::query_as!(
            StorageProtocolVersion,
            r#"
            SELECT
                *
            FROM
                protocol_versions
            ORDER BY
                id DESC
            LIMIT
                1
            "#,
        )
        .instrument("get_latest_protocol_version")
        .fetch_one(self.storage)
        .await?;

        Ok(ProtocolVersion::from(storage_protocol_version))
    }
}
