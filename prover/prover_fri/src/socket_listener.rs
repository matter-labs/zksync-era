use queues::IsQueue;
use std::net::SocketAddr;
use std::time::Instant;
use zksync_dal::ConnectionPool;
use zksync_types::proofs::{GpuProverInstanceStatus, SocketAddress};

use crate::utils::SharedWitnessVectorQueue;
use tokio::{
    io::copy,
    net::{TcpListener, TcpStream},
};
use tokio::sync::watch;
use zksync_object_store::bincode;
use zksync_prover_fri_types::WitnessVectorArtifacts;

pub(crate) struct SocketListener {
    address: SocketAddress,
    queue: SharedWitnessVectorQueue,
    pool: ConnectionPool,
    specialized_prover_group_id: u8,
    zone: String,
}

impl SocketListener {

    pub fn new(
        address: SocketAddress,
        queue: SharedWitnessVectorQueue,
        pool: ConnectionPool,
        specialized_prover_group_id: u8,
        zone: String,
    ) -> Self {
        Self {
            address,
            queue,
            pool,
            specialized_prover_group_id,
            zone,
        }
    }
    async fn init(&self) -> TcpListener {
        let listening_address = SocketAddr::new(self.address.host, self.address.port);
        vlog::info!(
            "Starting assembly receiver at host: {}, port: {}",
            self.address.host,
            self.address.port
        );
        let listener = TcpListener::bind(listening_address)
            .await
            .unwrap_or_else(|_| panic!("Failed binding address: {:?}", listening_address));

        let _lock = self.queue.lock().await;
        self.pool
            .access_storage()
            .await
            .fri_gpu_prover_queue_dal()
            .insert_prover_instance(
                self.address.clone(),
                self.specialized_prover_group_id,
                self.zone.clone(),
            )
            .await;
        listener
    }

    pub async fn listen_incoming_connections(self, stop_receiver: watch::Receiver<bool>) {
        let listener = self.init().await;
        let mut now = Instant::now();
        loop {
            if *stop_receiver.borrow() {
                vlog::warn!("Stop signal received, shutting down socket listener");
                return;
            }
            let stream = match listener.accept().await {
                Ok(stream) => stream.0,
                Err(e) => {
                    panic!("could not accept connection: {:?}", e);
                }
            };
            vlog::trace!(
                "Received new assembly send connection, waited for {}ms.",
                now.elapsed().as_millis()
            );

            self.handle_incoming_file(stream).await;

            now = Instant::now();
        }
    }

    async fn handle_incoming_file(&self, mut stream: TcpStream) {
        let mut assembly: Vec<u8> = vec![];
        let started_at = Instant::now();
        copy(&mut stream, &mut assembly)
            .await
            .expect("Failed reading from stream");
        let file_size_in_gb = assembly.len() / (1024 * 1024 * 1024);
        vlog::trace!(
            "Read file of size: {}GB from stream took: {} seconds",
            file_size_in_gb,
            started_at.elapsed().as_secs()
        );
        metrics::histogram!(
                "prover_fri.prover_fri.witness_vector_blob_time",
                started_at.elapsed(),
                "blob_size_in_gb" => file_size_in_gb.to_string(),
        );
        let witness_vector = bincode::deserialize::<WitnessVectorArtifacts>(&assembly)
            .expect("Failed deserializing witness vector");
        // acquiring lock from queue and updating db must be done atomically otherwise it results in TOCTTOU
        // Time-of-Check to Time-of-Use
        let mut queue = self.queue.lock().await;

        queue
            .add(witness_vector)
            .expect("Failed saving witness vector to queue");
        let status = if queue.capacity() == queue.size() {
            GpuProverInstanceStatus::Full
        } else {
            GpuProverInstanceStatus::Available
        };

        self.pool
            .access_storage()
            .await
            .fri_gpu_prover_queue_dal()
            .update_prover_instance_status(self.address.clone(), status, self.zone.clone())
            .await;
    }
}
