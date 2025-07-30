use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{server_notification::GatewayMigrationNotification, L1BlockNumber, H256};

use crate::{models::server_notification::ServerNotification, Core};

#[derive(Debug)]
pub struct ExternalNodeConfigDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ExternalNodeConfigDal<'_, '_> {
    pub async fn save_config(&mut self, value: serde_json::Value) -> DalResult<()> {
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
        let topics: Vec<&[u8]> = topics.iter().map(H256::as_bytes).collect();

        let rows = sqlx::query!(
            r#"
            SELECT
                *
            FROM
                server_notifications
            WHERE
                main_topic = ANY($1)
            ORDER BY
                l1_block_number DESC
            LIMIT 1
            "#,
            &topics as &[&[u8]]
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

    pub async fn get_latest_gateway_migration_notification(
        &mut self,
    ) -> DalResult<Option<GatewayMigrationNotification>> {
        let notification = self
            .get_last_notification_by_topics(
                &GatewayMigrationNotification::get_server_notifier_topics(),
            )
            .await?;

        let notification = notification.map(|notification| {
            GatewayMigrationNotification::from_topic(notification.main_topic)
                .expect("Invalid topic")
        });

        Ok(notification)
    }
}
