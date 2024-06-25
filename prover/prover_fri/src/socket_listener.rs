#[cfg(feature = "gpu")]
pub mod gpu_socket_listener {
    use std::{net::SocketAddr, sync::Arc, time::Instant};

    use anyhow::Context as _;
    use tokio::{
        io::copy,
        net::{TcpListener, TcpStream},
        sync::{watch, Notify},
    };
    use zksync_object_store::bincode;
    use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
    use zksync_prover_fri_types::WitnessVectorArtifacts;
    use zksync_types::{
        protocol_version::ProtocolSemanticVersion,
        prover_dal::{GpuProverInstanceStatus, SocketAddress},
    };

    use crate::{
        metrics::METRICS,
        utils::{GpuProverJob, SharedWitnessVectorQueue},
    };

    pub(crate) struct SocketListener {
        address: SocketAddress,
        queue: SharedWitnessVectorQueue,
        pool: ConnectionPool<Prover>,
        specialized_prover_group_id: u8,
        zone: String,
        protocol_version: ProtocolSemanticVersion,
    }

    impl SocketListener {
        pub fn new(
            address: SocketAddress,
            queue: SharedWitnessVectorQueue,
            pool: ConnectionPool<Prover>,
            specialized_prover_group_id: u8,
            zone: String,
            protocol_version: ProtocolSemanticVersion,
        ) -> Self {
            Self {
                address,
                queue,
                pool,
                specialized_prover_group_id,
                zone,
                protocol_version,
            }
        }
        async fn init(&self, init_notifier: Arc<Notify>) -> anyhow::Result<TcpListener> {
            let listening_address = SocketAddr::new(self.address.host, self.address.port);
            tracing::info!(
                "Starting assembly receiver at host: {}, port: {}",
                self.address.host,
                self.address.port
            );
            let listener = TcpListener::bind(listening_address)
                .await
                .with_context(|| format!("Failed binding address: {listening_address:?}"))?;

            let _lock = self.queue.lock().await;
            self.pool
                .connection()
                .await
                .unwrap()
                .fri_gpu_prover_queue_dal()
                .insert_prover_instance(
                    self.address.clone(),
                    self.specialized_prover_group_id,
                    self.zone.clone(),
                    self.protocol_version,
                )
                .await;
            init_notifier.notify_one();
            Ok(listener)
        }

        pub async fn listen_incoming_connections(
            self,
            stop_receiver: watch::Receiver<bool>,
            init_notifier: Arc<Notify>,
        ) -> anyhow::Result<()> {
            let listener = self.init(init_notifier).await.context("init()")?;
            let mut now = Instant::now();
            loop {
                if *stop_receiver.borrow() {
                    tracing::warn!("Stop signal received, shutting down socket listener");
                    return Ok(());
                }
                let stream = listener
                    .accept()
                    .await
                    .context("could not accept connection")?
                    .0;
                tracing::info!(
                    "Received new witness vector generator connection, waited for {:?}.",
                    now.elapsed()
                );

                self.handle_incoming_file(stream)
                    .await
                    .context("handle_incoming_file()")?;

                now = Instant::now();
            }
        }

        async fn handle_incoming_file(&self, mut stream: TcpStream) -> anyhow::Result<()> {
            let mut assembly: Vec<u8> = vec![];
            let started_at = Instant::now();
            copy(&mut stream, &mut assembly)
                .await
                .context("Failed reading from stream")?;
            let file_size_in_gb = assembly.len() / (1024 * 1024 * 1024);
            tracing::info!(
                "Read file of size: {}GB from stream after {:?}",
                file_size_in_gb,
                started_at.elapsed()
            );

            METRICS.witness_vector_blob_time[&(file_size_in_gb as u64)]
                .observe(started_at.elapsed());

            let witness_vector = bincode::deserialize::<WitnessVectorArtifacts>(&assembly)
                .context("Failed deserializing witness vector")?;
            tracing::info!(
                "Deserialized witness vector after {:?}",
                started_at.elapsed()
            );
            tracing::info!("Generated assembly after {:?}", started_at.elapsed());
            let gpu_prover_job = GpuProverJob {
                witness_vector_artifacts: witness_vector,
            };
            // acquiring lock from queue and updating db must be done atomically otherwise it results in `TOCTTOU`
            // Time-of-Check to Time-of-Use
            let mut queue = self.queue.lock().await;

            queue
                .add(gpu_prover_job)
                .map_err(|err| anyhow::anyhow!("Failed saving witness vector to queue: {err}"))?;
            tracing::info!(
                "Added witness vector to queue after {:?}",
                started_at.elapsed()
            );
            let status = if queue.capacity() == queue.size() {
                GpuProverInstanceStatus::Full
            } else {
                GpuProverInstanceStatus::Available
            };

            self.pool
                .connection()
                .await
                .unwrap()
                .fri_gpu_prover_queue_dal()
                .update_prover_instance_status(self.address.clone(), status, self.zone.clone())
                .await;
            tracing::info!(
                "Marked prover as {:?} after {:?}",
                status,
                started_at.elapsed()
            );
            Ok(())
        }
    }
}
