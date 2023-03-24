use std::io::Cursor;
use std::io::Read;
use std::sync::{Arc, Mutex};

use prover_service::RemoteSynthesizer;
use queues::{Buffer, IsQueue};
use zksync_dal::gpu_prover_queue_dal::SocketAddress;
use zksync_dal::ConnectionPool;

pub type SharedAssemblyQueue = Arc<Mutex<Buffer<Vec<u8>>>>;

pub struct SynthesizedCircuitProvider {
    queue: SharedAssemblyQueue,
    pool: ConnectionPool,
    address: SocketAddress,
}

impl SynthesizedCircuitProvider {
    pub fn new(queue: SharedAssemblyQueue, pool: ConnectionPool, address: SocketAddress) -> Self {
        Self {
            queue,
            pool,
            address,
        }
    }
}

impl RemoteSynthesizer for SynthesizedCircuitProvider {
    fn try_next(&mut self) -> Option<Box<dyn Read + Send + Sync>> {
        let mut assembly_queue = self.queue.lock().unwrap();
        let is_full = assembly_queue.capacity() == assembly_queue.size();
        return match assembly_queue.remove() {
            Ok(blob) => {
                let queue_free_slots = assembly_queue.capacity() - assembly_queue.size();
                if is_full {
                    self.pool
                        .clone()
                        .access_storage_blocking()
                        .gpu_prover_queue_dal()
                        .update_prover_instance_from_full_to_available(
                            self.address.clone(),
                            queue_free_slots,
                        );
                }
                vlog::info!(
                    "Queue free slot {} for capacity {}",
                    queue_free_slots,
                    assembly_queue.capacity()
                );
                metrics::histogram!(
                    "server.prover.queue_free_slots",
                    queue_free_slots as f64,
                    "queue_capacity" => assembly_queue.capacity().to_string()
                );
                Some(Box::new(Cursor::new(blob)))
            }
            Err(_) => None,
        };
    }
}
