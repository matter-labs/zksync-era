use zksync_types::api::ProtocolVersion;

use crate::connection::holder::Acquire;
use crate::models::storage_protocol_version::StorageProtocolVersion;
use crate::StorageProcessor;

#[derive(Debug)]
pub struct ProtocolVersionsWeb3Dal<'a, Conn: Acquire> {
    pub storage: &'a mut StorageProcessor<Conn>,
}

impl<'a, Conn: Acquire> ProtocolVersionsWeb3Dal<'a, Conn> {
    pub async fn get_protocol_version_by_id(&mut self, version_id: u16) -> Option<ProtocolVersion> {
        let storage_protocol_version: Option<StorageProtocolVersion> = sqlx::query_as!(
            StorageProtocolVersion,
            "SELECT * FROM protocol_versions
            WHERE id = $1
            ",
            version_id as i32
        )
        .fetch_optional(self.storage.acquire().await.as_conn())
        .await
        .unwrap();

        storage_protocol_version.map(ProtocolVersion::from)
    }

    pub async fn get_latest_protocol_version(&mut self) -> ProtocolVersion {
        let storage_protocol_version: StorageProtocolVersion = sqlx::query_as!(
            StorageProtocolVersion,
            "SELECT * FROM protocol_versions ORDER BY id DESC LIMIT 1",
        )
        .fetch_one(self.storage.acquire().await.as_conn())
        .await
        .unwrap();

        ProtocolVersion::from(storage_protocol_version)
    }
}
