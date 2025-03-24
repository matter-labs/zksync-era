use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{server_notification::ServerNotification, L1BlockNumber, H256};

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
        .instrument("save_notification")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn notifications_by_topics(
        &mut self,
        topics: Vec<H256>,
    ) -> DalResult<Vec<ServerNotification>> {
        let topics: Vec<Vec<u8>> = topics.into_iter().map(|a| a.as_bytes().to_vec()).collect();

        let rows = sqlx::query!(
            r#"
            SELECT
                *
            FROM
                server_notifications
            WHERE
                main_topic IN (SELECT unnest($1::bytea []))
            ORDER BY
                id DESC
            "#,
            &topics
        )
        .instrument("notifications_by_topics")
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|a| ServerNotification {
            l1_block_number: L1BlockNumber(a.l1_block_number as u32),
            main_topic: H256::from_slice(&a.main_topic),
            value: a.value,
        })
        .collect();

        Ok(rows)
    }
}
