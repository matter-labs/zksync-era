#[cfg(feature = "gpu")]
pub mod gpu_socket_listener {
    use shivini::synthesis_utils::{
        init_base_layer_cs_for_repeated_proving, init_recursive_layer_cs_for_repeated_proving,
    };
    use std::net::SocketAddr;
    use std::time::Instant;
    use zksync_dal::ConnectionPool;
    use zksync_types::proofs::AggregationRound;
    use zksync_types::proofs::{GpuProverInstanceStatus, SocketAddress};
    use zksync_vk_setup_data_server_fri::{
        get_finalization_hints, get_round_for_recursive_circuit_type,
    };

    use crate::utils::{GpuProverJob, ProvingAssembly, SharedWitnessVectorQueue};
    use anyhow::Context as _;
    use tokio::sync::watch;
    use tokio::{
        io::copy,
        net::{TcpListener, TcpStream},
    };
    use zksync_object_store::bincode;
    use zksync_prover_fri_types::{CircuitWrapper, ProverServiceDataKey, WitnessVectorArtifacts};

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
                tracing::trace!(
                    "Received new assembly send connection, waited for {}ms.",
                    now.elapsed().as_millis()
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
            tracing::trace!(
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
                .context("Failed deserializing witness vector")?;
            let assembly = generate_assembly_for_repeated_proving(
                witness_vector.prover_job.circuit_wrapper.clone(),
                witness_vector.prover_job.job_id,
                witness_vector.prover_job.setup_data_key.circuit_id,
            )
            .context("generate_assembly_for_repeated_proving()")?;
            let gpu_prover_job = GpuProverJob {
                witness_vector_artifacts: witness_vector,
                assembly,
            };
            // acquiring lock from queue and updating db must be done atomically otherwise it results in TOCTTOU
            // Time-of-Check to Time-of-Use
            let mut queue = self.queue.lock().await;

            queue
                .add(gpu_prover_job)
                .map_err(|err| anyhow::anyhow!("Failed saving witness vector to queue: {err}"))?;
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
        metrics::histogram!(
                "prover_fri.prover.gpu_assembly_generation_time",
                started_at.elapsed(),
                "circuit_type" => circuit_id.to_string()
        );
        Ok(cs)
    }
}
