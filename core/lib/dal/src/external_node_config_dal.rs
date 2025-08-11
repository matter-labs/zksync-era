use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};

use crate::Core;

#[derive(Debug)]
pub struct ExternalNodeConfigDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ExternalNodeConfigDal<'_, '_> {
    pub async fn save_config(&mut self, value: serde_json::Value) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            en_remote_config (
                value,
                updated_at
            )
            VALUES
            ($1, NOW())
            ON CONFLICT (id) DO UPDATE
            SET value = excluded.value,
            updated_at = NOW()
            "#,
            value
        )
        .instrument("save_en_remote_config")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_en_remote_config(&mut self) -> DalResult<Option<serde_json::Value>> {
        let row = sqlx::query!(
            r#"
            SELECT
                value
            FROM
                en_remote_config
            LIMIT 1
            "#,
        )
        .instrument("get_last_notification_by_topics")
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|a| a.value))
    }
}
