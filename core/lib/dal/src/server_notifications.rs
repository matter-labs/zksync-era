use sqlx::Row;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::{
    server_notification::{
        GatewayMigrationNotification, ProofSystemStatusNotification,
        PROOF_SYSTEM_STATUS_NOTIFICATION_SIGNATURE,
    },
    L1BlockNumber, H256,
};

use crate::{models::server_notification::ServerNotification, Core};

#[derive(Debug)]
pub struct ServerNotificationsDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl ServerNotificationsDal<'_, '_> {
    pub async fn save_proof_system_status_notification(
        &mut self,
        l1_block_number: L1BlockNumber,
        notification: ProofSystemStatusNotification,
    ) -> DalResult<()> {
        sqlx::query(
            r#"
            INSERT INTO server_notifications (main_topic, l1_block_number, value, created_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (main_topic, l1_block_number) DO UPDATE SET value = EXCLUDED.value
            WHERE (server_notifications.value->>'log_index')::bigint < (EXCLUDED.value->>'log_index')::bigint
            "#,
        )
        .bind(PROOF_SYSTEM_STATUS_NOTIFICATION_SIGNATURE.as_bytes())
        .bind(i64::from(l1_block_number.0))
        .bind(serde_json::to_value(notification).unwrap())
        .instrument("save_proof_system_status_notification")
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn get_latest_proof_system_status_notification(
        &mut self,
    ) -> DalResult<Option<ProofSystemStatusNotification>> {
        let value: Option<serde_json::Value> = sqlx::query(
            "SELECT value FROM server_notifications WHERE main_topic = $1 ORDER BY l1_block_number DESC LIMIT 1",
        )
        .bind(PROOF_SYSTEM_STATUS_NOTIFICATION_SIGNATURE.as_bytes())
        .try_map(|row: sqlx::postgres::PgRow| row.try_get(0))
        .instrument("get_latest_proof_system_status_notification")
        .fetch_optional(self.storage)
        .await?;
        value
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| {
                Instrumented::new("get_latest_proof_system_status_notification")
                    .constraint_error(err.into())
            })
    }

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

    pub async fn get_latest_gateway_migration_notification_and_block_number(
        &mut self,
    ) -> DalResult<Option<(GatewayMigrationNotification, L1BlockNumber)>> {
        let notification = self
            .get_last_notification_by_topics(
                &GatewayMigrationNotification::get_server_notifier_topics(),
            )
            .await?;

        let notification = notification.map(|notification| {
            (
                GatewayMigrationNotification::from_topic(notification.main_topic)
                    .expect("Invalid topic"),
                notification.l1_block_number,
            )
        });

        Ok(notification)
    }
}
