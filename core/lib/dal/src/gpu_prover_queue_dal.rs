use std::net::IpAddr;
use std::time::Duration;

use crate::time_utils::pg_interval_from_duration;
use crate::StorageProcessor;

#[derive(Debug)]
pub struct GpuProverQueueDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

#[derive(Debug, Clone)]
pub struct SocketAddress {
    pub host: IpAddr,
    pub port: u16,
}

#[derive(Debug)]
pub enum GpuProverInstanceStatus {
    // The instance is available for processing.
    Available,
    // The instance is running at full capacity.
    Full,
    // The instance is reserved by an synthesizer.
    Reserved,
    // The instance is not alive anymore.
    Dead,
}

impl GpuProverQueueDal<'_, '_> {
    pub fn get_free_prover_instance(
        &mut self,
        processing_timeout: Duration,
        specialized_prover_group_id: u8,
        region: String,
    ) -> Option<SocketAddress> {
        async_std::task::block_on(async {
            let processing_timeout = pg_interval_from_duration(processing_timeout);
            let result: Option<SocketAddress> = sqlx::query!(
                "
                UPDATE gpu_prover_queue
                SET instance_status = 'reserved',
                    updated_at = now(),
                    processing_started_at = now()
                WHERE (instance_host, instance_port) in (
                    SELECT instance_host, instance_port
                    FROM gpu_prover_queue
                    WHERE specialized_prover_group_id=$2
                    AND region=$3
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
                region
            )
                .fetch_optional(self.storage.conn())
                .await
                .unwrap()
                .map(|row| SocketAddress {
                    host: row.instance_host.network(),
                    port: row.instance_port as u16,
                });

            result
        })
    }

    pub fn insert_prover_instance(
        &mut self,
        address: SocketAddress,
        queue_capacity: usize,
        specialized_prover_group_id: u8,
        region: String,
    ) {
        async_std::task::block_on(async {
            sqlx::query!(
                    "
                    INSERT INTO gpu_prover_queue (instance_host, instance_port, queue_capacity, queue_free_slots, instance_status, specialized_prover_group_id, region, created_at, updated_at)
                    VALUES (cast($1::text as inet), $2, $3, $3, 'available', $4, $5, now(), now())
                    ON CONFLICT(instance_host, instance_port, region)
                    DO UPDATE SET instance_status='available', queue_capacity=$3, queue_free_slots=$3, specialized_prover_group_id=$4, region=$5, updated_at=now()",
                    format!("{}",address.host),
                address.port as i32,
                queue_capacity as i32,
                specialized_prover_group_id as i16,
                region)
                .execute(self.storage.conn())
                .await
                .unwrap();
        })
    }

    pub fn update_prover_instance_status(
        &mut self,
        address: SocketAddress,
        status: GpuProverInstanceStatus,
        queue_free_slots: usize,
    ) {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                UPDATE gpu_prover_queue
                SET instance_status = $1, updated_at = now(), queue_free_slots = $4
                WHERE instance_host = $2::text::inet
                AND instance_port = $3
                ",
                format!("{:?}", status).to_lowercase(),
                format!("{}", address.host),
                address.port as i32,
                queue_free_slots as i32,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn update_prover_instance_from_full_to_available(
        &mut self,
        address: SocketAddress,
        queue_free_slots: usize,
    ) {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                UPDATE gpu_prover_queue
                SET instance_status = 'available', updated_at = now(), queue_free_slots = $3
                WHERE instance_host = $1::text::inet
                AND instance_port = $2
                AND instance_status = 'full'
                ",
                format!("{}", address.host),
                address.port as i32,
                queue_free_slots as i32
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_count_of_jobs_ready_for_processing(&mut self) -> u32 {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                SELECT MIN(count) as "count"
                FROM (SELECT COALESCE(SUM(queue_free_slots), 0) as "count"
                      FROM gpu_prover_queue
                      where instance_status = 'available'
                      UNION
                      SELECT count(*) as "count"
                      from prover_jobs
                      where status = 'queued'
                     ) as t1;
               "#,
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count
            .unwrap() as u32
        })
    }
}
