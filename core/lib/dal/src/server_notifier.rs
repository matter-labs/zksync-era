use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{L1BlockNumber, H256};

use crate::Core;

#[derive(Debug)]
pub struct ServerNotificationsDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ServerNotificationsDal<'_, '_> {
    pub async fn save_notification(
        &mut self,
        main_topic: H256,
        l1_block_number: L1BlockNumber,
        value: serde_json::Value,
    ) -> DalResult<()> {
        let mut db_transaction = self.storage.start_transaction().await?;
        sqlx::query!(
            r#"
            INSERT INTO
            server_notifications (
                main_topic,
                l1_block_number,
                value,
                created_at
            )
            VALUES
            ($1, $2, $3, NOW())
            ON CONFLICT DO NOTHING
            "#,
            main_topic.as_bytes(),
            l1_block_number.0 as i32,
            value
        )
        .instrument("save_protocol_version#minor")
        .execute(&mut db_transaction)
        .await?;

        db_transaction.commit().await?;

        Ok(())
    }

    pub async fn notifications_by_topic(&mut self, main_topic: H256) -> sqlx::Result<()> {
        let row = sqlx::query!(
            r#"
            SELECT
                *
            FROM
                server_notifications
            WHERE
                main_topic = $1
            ORDER BY
                id DESC
            "#,
            main_topic.as_bytes()
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(())
    }
}
