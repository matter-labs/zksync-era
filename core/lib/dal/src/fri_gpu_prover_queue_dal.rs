use std::time::Duration;
use zksync_types::proofs::{GpuProverInstanceStatus, SocketAddress};

use crate::time_utils::pg_interval_from_duration;
use crate::StorageProcessor;

#[derive(Debug)]
pub struct FriGpuProverQueueDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
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
                "UPDATE gpu_prover_queue_fri \
                SET instance_status = 'reserved', \
                    updated_at = now(), \
                    processing_started_at = now() \
                WHERE id in ( \
                    SELECT id \
                    FROM gpu_prover_queue_fri \
                    WHERE specialized_prover_group_id=$2 \
                    AND zone=$3 \
                    AND ( \
                        instance_status = 'available' \
                        OR (instance_status = 'reserved' AND  processing_started_at < now() - $1::interval) \
                    ) \
                    ORDER BY updated_at ASC \
                    LIMIT 1 \
                    FOR UPDATE \
                    SKIP LOCKED \
                ) \
                RETURNING gpu_prover_queue_fri.*
                ",
                &processing_timeout,
                specialized_prover_group_id as i16,
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
            "INSERT INTO gpu_prover_queue_fri (instance_host, instance_port, instance_status, specialized_prover_group_id,  zone, created_at, updated_at) \
             VALUES (cast($1::text as inet), $2, 'available', $3, $4, now(), now()) \
             ON CONFLICT(instance_host, instance_port, zone) \
             DO UPDATE SET instance_status='available', specialized_prover_group_id=$3, zone=$4, updated_at=now()",
             format!("{}",address.host),
             address.port as i32,
             specialized_prover_group_id as i16,
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
            "UPDATE gpu_prover_queue_fri \
                SET instance_status = $1, updated_at = now() \
                WHERE instance_host = $2::text::inet \
                AND instance_port = $3 \
                AND zone = $4
                ",
            format!("{:?}", status).to_lowercase(),
            format!("{}", address.host),
            address.port as i32,
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
            "UPDATE gpu_prover_queue_fri \
                SET instance_status = 'available', updated_at = now() \
                WHERE instance_host = $1::text::inet \
                AND instance_port = $2 \
                AND instance_status = 'full' \
                AND zone = $3
                ",
            format!("{}", address.host),
            address.port as i32,
            zone
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }
}
