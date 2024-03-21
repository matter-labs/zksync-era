use std::time::Duration;

use zksync_basic_types::prover_dal::{GpuProverInstanceStatus, SocketAddress};
use zksync_db_connection::connection::Connection;

use crate::{pg_interval_from_duration, Prover};

#[derive(Debug)]
pub struct FriGpuProverQueueDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriGpuProverQueueDal<'_, '_> {
    pub async fn lock_available_prover(
        &mut self,
        processing_timeout: Duration,
        specialized_prover_group_id: u8,
        zone: String,
    ) -> Option<SocketAddress> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let result: Option<SocketAddress> = sqlx::query!(
            r#"
            UPDATE gpu_prover_queue_fri
            SET
                instance_status = 'reserved',
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                id IN (
                    SELECT
                        id
                    FROM
                        gpu_prover_queue_fri
                    WHERE
                        specialized_prover_group_id = $2
                        AND zone = $3
                        AND (
                            instance_status = 'available'
                            OR (
                                instance_status = 'reserved'
                                AND processing_started_at < NOW() - $1::INTERVAL
                            )
                        )
                    ORDER BY
                        updated_at ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                gpu_prover_queue_fri.*
            "#,
            &processing_timeout,
            i16::from(specialized_prover_group_id),
            zone
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| SocketAddress {
            host: row.instance_host.network(),
            port: row.instance_port as u16,
        });

        result
    }

    pub async fn insert_prover_instance(
        &mut self,
        address: SocketAddress,
        specialized_prover_group_id: u8,
        zone: String,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
                gpu_prover_queue_fri (
                    instance_host,
                    instance_port,
                    instance_status,
                    specialized_prover_group_id,
                    zone,
                    created_at,
                    updated_at
                )
            VALUES
                (CAST($1::TEXT AS inet), $2, 'available', $3, $4, NOW(), NOW())
            ON CONFLICT (instance_host, instance_port, zone) DO
            UPDATE
            SET
                instance_status = 'available',
                specialized_prover_group_id = $3,
                zone = $4,
                updated_at = NOW()
            "#,
            address.host.to_string(),
            i32::from(address.port),
            i16::from(specialized_prover_group_id),
            zone
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn update_prover_instance_status(
        &mut self,
        address: SocketAddress,
        status: GpuProverInstanceStatus,
        zone: String,
    ) {
        sqlx::query!(
            r#"
            UPDATE gpu_prover_queue_fri
            SET
                instance_status = $1,
                updated_at = NOW()
            WHERE
                instance_host = $2::TEXT::inet
                AND instance_port = $3
                AND zone = $4
            "#,
            format!("{status:?}").to_lowercase(),
            address.host.to_string(),
            i32::from(address.port),
            zone
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn update_prover_instance_from_full_to_available(
        &mut self,
        address: SocketAddress,
        zone: String,
    ) {
        sqlx::query!(
            r#"
            UPDATE gpu_prover_queue_fri
            SET
                instance_status = 'available',
                updated_at = NOW()
            WHERE
                instance_host = $1::TEXT::inet
                AND instance_port = $2
                AND instance_status = 'full'
                AND zone = $3
            "#,
            address.host.to_string(),
            i32::from(address.port),
            zone
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }
}
