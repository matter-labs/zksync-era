use std::{
    io::{Cursor, Read},
    sync::Arc,
};

use prover_service::RemoteSynthesizer;
use queues::{Buffer, IsQueue};
use tokio::{runtime::Handle, sync::Mutex};
use zksync_dal::ConnectionPool;
use zksync_types::proofs::SocketAddress;

use crate::metrics::METRICS;

pub type SharedAssemblyQueue = Arc<Mutex<Buffer<Vec<u8>>>>;

pub struct SynthesizedCircuitProvider {
    rt_handle: Handle,
    queue: SharedAssemblyQueue,
    pool: ConnectionPool,
    address: SocketAddress,
    region: String,
    zone: String,
}

impl SynthesizedCircuitProvider {
    pub fn new(
        queue: SharedAssemblyQueue,
        pool: ConnectionPool,
        address: SocketAddress,
        region: String,
        zone: String,
        rt_handle: Handle,
    ) -> Self {
        Self {
            rt_handle,
            queue,
            pool,
            address,
            region,
            zone,
        }
    }
}

impl RemoteSynthesizer for SynthesizedCircuitProvider {
    fn try_next(&mut self) -> Option<Box<dyn Read + Send + Sync>> {
        let mut assembly_queue = self.rt_handle.block_on(async { self.queue.lock().await });
        let is_full = assembly_queue.capacity() == assembly_queue.size();
        return match assembly_queue.remove() {
            Ok(blob) => {
                let queue_free_slots = assembly_queue.capacity() - assembly_queue.size();
                if is_full {
                    self.rt_handle.block_on(async {
                        self.pool
                            .access_storage()
                            .await
                            .unwrap()
                            .gpu_prover_queue_dal()
                            .update_prover_instance_from_full_to_available(
                                self.address.clone(),
                                queue_free_slots,
                                self.region.clone(),
                                self.zone.clone(),
                            )
                            .await
                    });
                }
                tracing::trace!(
                    "Queue free slot {} for capacity {}",
                    queue_free_slots,
                    assembly_queue.capacity()
                );
                METRICS.queue_free_slots[&assembly_queue.capacity().to_string()]
                    .observe(queue_free_slots);

                Some(Box::new(Cursor::new(blob)))
            }
            Err(_) => None,
        };
    }
}
