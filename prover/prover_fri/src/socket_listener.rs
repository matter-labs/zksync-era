#[cfg(feature = "gpu")]
pub mod gpu_socket_listener {
    use std::{net::SocketAddr, time::Instant};

    use anyhow::Context as _;
    use shivini::synthesis_utils::{
        init_base_layer_cs_for_repeated_proving, init_recursive_layer_cs_for_repeated_proving,
    };
    use tokio::{
        io::copy,
        net::{TcpListener, TcpStream},
        sync::watch,
    };
    use zksync_dal::ConnectionPool;
    use zksync_object_store::bincode;
    use zksync_prover_fri_types::{CircuitWrapper, ProverServiceDataKey, WitnessVectorArtifacts};
    use zksync_types::proofs::{AggregationRound, GpuProverInstanceStatus, SocketAddress};
    use zksync_vk_setup_data_server_fri::{
        get_finalization_hints, get_round_for_recursive_circuit_type,
    };

    use crate::{
        metrics::METRICS,
        utils::{GpuProverJob, ProvingAssembly, SharedWitnessVectorQueue},
    };

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
        async fn init(&self) -> anyhow::Result<TcpListener> {
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
                .access_storage()
                .await
                .unwrap()
                .fri_gpu_prover_queue_dal()
                .insert_prover_instance(
                    self.address.clone(),
                    self.specialized_prover_group_id,
                    self.zone.clone(),
                )
                .await;
            Ok(listener)
        }

        pub async fn listen_incoming_connections(
            self,
            stop_receiver: watch::Receiver<bool>,
        ) -> anyhow::Result<()> {
            let listener = self.init().await.context("init()")?;
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
            let assembly = generate_assembly_for_repeated_proving(
                witness_vector.prover_job.circuit_wrapper.clone(),
                witness_vector.prover_job.job_id,
                witness_vector.prover_job.setup_data_key.circuit_id,
            )
            .context("generate_assembly_for_repeated_proving()")?;
            tracing::info!("Generated assembly after {:?}", started_at.elapsed());
            let gpu_prover_job = GpuProverJob {
                witness_vector_artifacts: witness_vector,
                assembly,
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
                .access_storage()
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

    pub fn generate_assembly_for_repeated_proving(
        circuit_wrapper: CircuitWrapper,
        job_id: u32,
        circuit_id: u8,
    ) -> anyhow::Result<ProvingAssembly> {
        let started_at = Instant::now();
        let cs = match circuit_wrapper {
            CircuitWrapper::Base(base_circuit) => {
                let key = ProverServiceDataKey::new(
                    base_circuit.numeric_circuit_type(),
                    AggregationRound::BasicCircuits,
                );
                let finalization_hint =
                    get_finalization_hints(key).context("get_finalization_hints()")?;
                init_base_layer_cs_for_repeated_proving(base_circuit, &finalization_hint)
            }
            CircuitWrapper::Recursive(recursive_circuit) => {
                let key = ProverServiceDataKey::new(
                    recursive_circuit.numeric_circuit_type(),
                    get_round_for_recursive_circuit_type(recursive_circuit.numeric_circuit_type()),
                );
                let finalization_hint =
                    get_finalization_hints(key).context("get_finalization_hints()")?;
                init_recursive_layer_cs_for_repeated_proving(recursive_circuit, &finalization_hint)
            }
        };
        tracing::info!(
            "Successfully generated assembly without witness vector for job: {}, took: {:?}",
            job_id,
            started_at.elapsed()
        );

        METRICS.gpu_assembly_generation_time[&circuit_id.to_string()].observe(started_at.elapsed());

        Ok(cs)
    }
}
