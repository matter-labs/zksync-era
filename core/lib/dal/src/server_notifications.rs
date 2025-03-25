use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{L1BlockNumber, H256};

use crate::{models::server_notification::ServerNotification, Core};

#[derive(Debug)]
pub struct ServerNotificationsDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ServerNotificationsDal<'_, '_> {
    pub async fn save_notification(
        &mut self,
        main_topic: H256,
        l1_block_number: L1BlockNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            server_notifications (
                main_topic,
                l1_block_number,
                created_at
            )
            VALUES
            ($1, $2, NOW())
            ON CONFLICT DO NOTHING
            "#,
            main_topic.as_bytes(),
            l1_block_number.0 as i32,
        )
        .instrument("save_notification")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_last_notification_by_topics(
        &mut self,
        topics: &[H256],
    ) -> DalResult<Option<ServerNotification>> {
        let topics: Vec<Vec<u8>> = topics.iter().map(|a| a.as_bytes().to_vec()).collect();

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
            LIMIT 1
            "#,
            &topics
        )
        .instrument("get_last_notification_by_topics")
        .fetch_optional(self.storage)
        .await?
        .map(|a| ServerNotification {
            l1_block_number: L1BlockNumber(a.l1_block_number as u32),
            main_topic: H256::from_slice(&a.main_topic),
        });

        Ok(rows)
    }
}
