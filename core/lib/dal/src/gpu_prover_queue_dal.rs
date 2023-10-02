use std::time::Duration;

use crate::time_utils::pg_interval_from_duration;
use crate::StorageProcessor;
use std::collections::HashMap;
use zksync_types::proofs::{GpuProverInstanceStatus, SocketAddress};

#[derive(Debug)]
pub struct GpuProverQueueDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl GpuProverQueueDal<'_, '_> {
    pub async fn lock_available_prover(
        &mut self,
        processing_timeout: Duration,
        specialized_prover_group_id: u8,
        region: String,
        zone: String,
    ) -> Option<SocketAddress> {
        {
            let processing_timeout = pg_interval_from_duration(processing_timeout);
            let result: Option<SocketAddress> = sqlx::query!(
                "
                UPDATE gpu_prover_queue
                SET instance_status = 'reserved',
                    updated_at = now(),
                    processing_started_at = now()
                WHERE id in (
                    SELECT id
                    FROM gpu_prover_queue
                    WHERE specialized_prover_group_id=$2
                    AND region=$3
                    AND zone=$4
                    AND (
                        instance_status = 'available'
                        OR (instance_status = 'reserved' AND  processing_started_at < now() - $1::interval)
                    )
                    ORDER BY updated_at ASC
                    LIMIT 1
                    FOR UPDATE
                    SKIP LOCKED
                )
                RETURNING gpu_prover_queue.*
                ",
                &processing_timeout,
                specialized_prover_group_id as i16,
                region,
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
    }

    pub async fn insert_prover_instance(
        &mut self,
        address: SocketAddress,
        queue_capacity: usize,
        specialized_prover_group_id: u8,
        region: String,
        zone: String,
        num_gpu: u8,
    ) {
        {
            sqlx::query!(
                    "
                    INSERT INTO gpu_prover_queue (instance_host, instance_port, queue_capacity, queue_free_slots, instance_status, specialized_prover_group_id, region, zone, num_gpu, created_at, updated_at)
                    VALUES (cast($1::text as inet), $2, $3, $3, 'available', $4, $5, $6, $7, now(), now())
                    ON CONFLICT(instance_host, instance_port, region, zone)
                    DO UPDATE SET instance_status='available', queue_capacity=$3, queue_free_slots=$3, specialized_prover_group_id=$4, region=$5, zone=$6, num_gpu=$7, updated_at=now()",
                    format!("{}",address.host),
                address.port as i32,
                queue_capacity as i32,
                specialized_prover_group_id as i16,
                region,
                zone,
                num_gpu as i16)
                .execute(self.storage.conn())
                .await
                .unwrap();
        }
    }

    pub async fn update_prover_instance_status(
        &mut self,
        address: SocketAddress,
        status: GpuProverInstanceStatus,
        queue_free_slots: usize,
        region: String,
        zone: String,
    ) {
        {
            sqlx::query!(
                "
                UPDATE gpu_prover_queue
                SET instance_status = $1, updated_at = now(), queue_free_slots = $4
                WHERE instance_host = $2::text::inet
                AND instance_port = $3
                AND region = $5
                AND zone = $6
                ",
                format!("{:?}", status).to_lowercase(),
                format!("{}", address.host),
                address.port as i32,
                queue_free_slots as i32,
                region,
                zone
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn update_prover_instance_from_full_to_available(
        &mut self,
        address: SocketAddress,
        queue_free_slots: usize,
        region: String,
        zone: String,
    ) {
        {
            sqlx::query!(
                "
                UPDATE gpu_prover_queue
                SET instance_status = 'available', updated_at = now(), queue_free_slots = $3
                WHERE instance_host = $1::text::inet
                AND instance_port = $2
                AND instance_status = 'full'
                AND region = $4
                AND zone = $5
                ",
                format!("{}", address.host),
                address.port as i32,
                queue_free_slots as i32,
                region,
                zone
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn get_prover_gpu_count_per_region_zone(&mut self) -> HashMap<(String, String), u64> {
        {
            sqlx::query!(
                r#"
                SELECT region, zone, SUM(num_gpu) AS total_gpus
                FROM gpu_prover_queue
                GROUP BY region, zone
               "#,
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| ((row.region, row.zone), row.total_gpus.unwrap() as u64))
            .collect()
        }
    }
}
