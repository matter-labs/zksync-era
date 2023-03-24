use std::io::copy;
use std::net::SocketAddr;
use std::net::TcpStream;
use std::time::Instant;

use local_ip_address::local_ip;
use prover_service::prover::{Prover, ProvingAssembly};
use prover_service::remote_synth::serialize_job;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::plonk::better_better_cs::cs::Circuit;
use zkevm_test_harness::pairing::bn256::Bn256;
use zkevm_test_harness::witness::oracle::VmWitnessOracle;

use zksync_config::configs::CircuitSynthesizerConfig;
use zksync_config::configs::prover_group::ProverGroupConfig;
use zksync_config::ProverConfigs;
use zksync_dal::ConnectionPool;
use zksync_dal::gpu_prover_queue_dal::{GpuProverInstanceStatus, SocketAddress};
use zksync_object_store::gcs_utils::prover_circuit_input_blob_url;
use zksync_object_store::object_store::{create_object_store_from_env, PROVER_JOBS_BUCKET_PATH};
use zksync_prover_utils::numeric_index_to_circuit_name;
use zksync_prover_utils::region_fetcher::get_region;
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::proofs::ProverJobMetadata;

pub struct CircuitSynthesizer {
    config: CircuitSynthesizerConfig,
}

impl CircuitSynthesizer {
    pub fn new(config: CircuitSynthesizerConfig) -> Self {
        Self { config }
    }

    pub fn synthesize(
        circuit: ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>,
    ) -> (ProvingAssembly, u8) {
        let circuit_synthesis_started_at = Instant::now();
        let mut assembly = Prover::new_proving_assembly();
        circuit
            .synthesize(&mut assembly)
            .expect("circuit synthesize failed");
        let circuit_type = numeric_index_to_circuit_name(circuit.numeric_circuit_type()).unwrap();
        vlog::info!(
            "Finished circuit synthesis for circuit: {} took {:?} seconds",
            circuit_type,
            circuit_synthesis_started_at.elapsed().as_secs(),
        );
        metrics::histogram!(
            "server.circuit_synthesizer.synthesize",
            circuit_synthesis_started_at.elapsed().as_secs() as f64,
            "circuit_type" => circuit_type,
        );

        // we don't perform assembly finalization here since it increases the assembly size significantly due to padding.
        (assembly, circuit.numeric_circuit_type())
    }
}

fn get_circuit(
    prover_job_metadata: ProverJobMetadata,
) -> ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>> {
    let circuit_input_blob_url = prover_circuit_input_blob_url(
        prover_job_metadata.block_number,
        prover_job_metadata.sequence_number,
        prover_job_metadata.circuit_type.clone(),
        prover_job_metadata.aggregation_round,
    );
    let object_store = create_object_store_from_env();
    let circuit_input = object_store
        .get(PROVER_JOBS_BUCKET_PATH, circuit_input_blob_url)
        .expect("Failed fetching prover jobs from GCS");

    bincode::deserialize::<ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>(&circuit_input)
        .expect("Failed to deserialize circuit input")
}

#[async_trait]
impl JobProcessor for CircuitSynthesizer {
    type Job = ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>;
    type JobId = u32;
    type JobArtifacts = (ProvingAssembly, u8);
    const SERVICE_NAME: &'static str = "CircuitSynthesizer";

    async fn get_next_job(
        &self,
        connection_pool: ConnectionPool,
    ) -> Option<(Self::JobId, Self::Job)> {
        let config: CircuitSynthesizerConfig = CircuitSynthesizerConfig::from_env();
        let prover_group_config = ProverGroupConfig::from_env();

        let circuit_ids = prover_group_config
            .get_circuit_ids_for_group_id(config.prover_group_id)
            .unwrap_or(vec![]);
        if prover_group_config.is_specialized_group_id(config.prover_group_id) {
            assert!(!circuit_ids.is_empty(), "No circuits found for specialized prover group id :{}", config.prover_group_id);
        }
        vlog::info!("Fetching prover jobs for group: {} and circuits: {:?}", config.prover_group_id, circuit_ids);
        let circuit_types: Vec<String> = circuit_ids.iter()
            .map(|&id| numeric_index_to_circuit_name(id).unwrap_or_else(|| panic!("unknown id :{}", id)).to_string())
            .collect();
        let prover_job = if circuit_types.is_empty() {
            connection_pool
                .access_storage_blocking()
                .prover_dal()
                .get_next_prover_job(self.config.generation_timeout(), self.config.max_attempts)?
        } else {
            connection_pool
                .access_storage_blocking()
                .prover_dal()
                .get_next_prover_job_by_circuit_types(self.config.generation_timeout(), self.config.max_attempts, circuit_types)?
        };
        let job_id = prover_job.id;
        Some((job_id, get_circuit(prover_job)))
    }

    async fn save_failure(
        pool: ConnectionPool,
        job_id: Self::JobId,
        _started_at: Instant,
        error: String,
    ) -> () {
        let config: CircuitSynthesizerConfig = CircuitSynthesizerConfig::from_env();
        pool.access_storage_blocking()
            .prover_dal()
            .save_proof_error(job_id, error, config.max_attempts);
    }

    async fn process_job(
        _connection_pool: ConnectionPool,
        job: Self::Job,
        _started_at: Instant,
    ) -> JoinHandle<Self::JobArtifacts> {
        tokio::task::spawn_blocking(move || Self::synthesize(job))
    }

    async fn save_result(
        pool: ConnectionPool,
        job_id: Self::JobId,
        _started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) {
        let region = get_region().await;
        vlog::info!("Finished circuit synthesis for job: {} in region: {}", job_id, region);
        let config: CircuitSynthesizerConfig = CircuitSynthesizerConfig::from_env();
        let (assembly, circuit_id) = artifacts;
        let now = Instant::now();
        while now.elapsed() < config.prover_instance_wait_timeout() {
            let optional_prover_instance = pool
                .clone()
                .access_storage_blocking()
                .gpu_prover_queue_dal()
                .get_free_prover_instance(config.gpu_prover_queue_timeout(), config.prover_group_id, region.clone());
            match optional_prover_instance {
                Some(address) => {
                    vlog::info!(
                        "Found a free prover instance: {:?} to send assembly for job: {}",
                        address,
                        job_id
                    );
                    send_assembly(job_id, circuit_id, assembly, address, pool);
                    return;
                }
                None => {
                    sleep(config.prover_instance_poll_time()).await;
                }
            }
        }
        vlog::info!(
            "Not able to get any free prover instance for sending assembly for job: {}",
            job_id
        );
    }
}

fn send_assembly(
    job_id: u32,
    circuit_id: u8,
    assembly: ProvingAssembly,
    address: SocketAddress,
    pool: ConnectionPool,
) {
    let socket_address = SocketAddr::new(address.host, address.port);
    vlog::info!(
        "Sending assembly to host: {}, port: {}",
        address.host,
        address.port
    );
    match TcpStream::connect(socket_address) {
        Ok(stream) => {
            serialize_and_send(job_id, circuit_id, address, stream, assembly, pool);
        }
        Err(e) => {
            vlog::info!(
                "Failed sending assembly to address: {:?}, socket not reachable reason: {:?}",
                address,
                e
            );
            handle_unreachable_prover_instance(job_id, address, pool);
        }
    }
}

fn serialize_and_send(
    job_id: u32,
    circuit_id: u8,
    address: SocketAddress,
    mut stream: TcpStream,
    assembly: ProvingAssembly,
    pool: ConnectionPool,
) {
    let started_at = Instant::now();
    let mut serialized: Vec<u8> = vec![];
    serialize_job::<_>(&assembly, job_id as usize, circuit_id, &mut serialized);
    let blob_size_in_gb = serialized.len() / (1024 * 1024 * 1024);
    copy(&mut serialized.as_slice(), &mut stream)
        .unwrap_or_else(|_| panic!("failed sending assembly to address: {:?}", address));
    let local_ip = local_ip().expect("Failed obtaining local IP address");
    vlog::info!(
        "Sent assembly of size: {}GB successfully, took: {} seconds for job: {} by: {:?} to: {:?}",
        blob_size_in_gb,
        started_at.elapsed().as_secs(),
        job_id,
        local_ip,
        address
    );
    metrics::histogram!(
        "server.circuit_synthesizer.blob_sending_time",
        started_at.elapsed().as_secs() as f64,
        "blob_size_in_gb" => blob_size_in_gb.to_string(),
    );
    handle_successful_sent_assembly(job_id, pool);
}

fn handle_successful_sent_assembly(job_id: u32, pool: ConnectionPool) {
    // releasing prover instance in gpu_prover_queue by marking it available is done by prover itself.
    // we don't do it here to avoid race condition.

    // mark the job as `in_gpu_proof`
    pool.clone()
        .access_storage_blocking()
        .prover_dal()
        .update_status(job_id, "in_gpu_proof");
}

fn handle_unreachable_prover_instance(job_id: u32, address: SocketAddress, pool: ConnectionPool) {
    // mark prover instance in gpu_prover_queue dead
    pool.clone()
        .access_storage_blocking()
        .gpu_prover_queue_dal()
        .update_prover_instance_status(address, GpuProverInstanceStatus::Dead, 0);

    let prover_config = ProverConfigs::from_env().non_gpu;
    // mark the job as failed
    pool.clone()
        .access_storage_blocking()
        .prover_dal()
        .save_proof_error(
            job_id,
            "prover instance unreachable".to_string(),
            prover_config.max_attempts,
        );
}
