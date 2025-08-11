use zksync_config::RemoteENConfig;
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::Instrumented};

use crate::Core;

#[derive(Debug)]
pub struct ExternalNodeConfigDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ExternalNodeConfigDal<'_, '_> {
    pub async fn save_config(&mut self, value: &RemoteENConfig) -> DalResult<()> {
        let instrumentation = Instrumented::new("save_en_config");

        let value =
            serde_json::to_value(value).map_err(|err| instrumentation.arg_error("value", err))?;
        let query = sqlx::query!(
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
        );
        instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;

        Ok(())
    }

    pub async fn get_en_remote_config(&mut self) -> DalResult<Option<RemoteENConfig>> {
        let instrumentation = Instrumented::new("get_en_remote_config");

        let query = sqlx::query!(
            r#"
            SELECT
                value
            FROM
                en_remote_config
            LIMIT 1
            "#,
        );

        let row = instrumentation
            .clone()
            .with(query)
            .fetch_optional(self.storage)
            .await?;
        row.map(|a| {
            serde_json::from_value(a.value)
                .map_err(|err| instrumentation.arg_error("RemoteENConfig", err))
        })
        .transpose()
    }
}
