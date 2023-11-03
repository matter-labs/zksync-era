use std::time::Duration;

use serde::Deserialize;

/// Configuration for the prover application
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProverConfig {
    /// Port to which the Prometheus exporter server is listening.
    pub prometheus_port: u16,
    /// Currently only a single (largest) key is supported. We'll support different ones in the future
    pub initial_setup_key_path: String,
    /// https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^26.key
    pub key_download_url: String,
    /// Max time for proof to be generated
    pub generation_timeout_in_secs: u16,
    /// Number of threads to be used concurrent proof generation.
    pub number_of_threads: u16,
    /// Max attempts for generating proof
    pub max_attempts: u32,
    // Polling time in mill-seconds.
    pub polling_duration_in_millis: u64,
    // Path to setup keys for individual circuit.
    pub setup_keys_path: String,
    // Group id for this prover, provers running the same circuit types shall have same group id.
    pub specialized_prover_group_id: u8,
    // Number of setup-keys kept in memory without swapping
    // number_of_setup_slots = (R-C*A-4)/S
    // R is available ram
    // C is the number of parallel synth
    // A is the size of Assembly that is 12gb
    // S is the size of the Setup that is 20gb
    // constant 4 is for the data copy with gpu
    pub number_of_setup_slots: u8,
    /// Port at which server would be listening to receive incoming assembly
    pub assembly_receiver_port: u16,
    /// Socket polling time for receiving incoming assembly
    pub assembly_receiver_poll_time_in_millis: u64,
    /// maximum number of assemblies that are kept in memory,
    pub assembly_queue_capacity: usize,
}

/// Prover configs for different machine types that are currently supported.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProverConfigs {
    // used by witness-generator
    pub non_gpu: ProverConfig,
    // https://gcloud-compute.com/a2-highgpu-2g.html
    pub two_gpu_forty_gb_mem: ProverConfig,
    // https://gcloud-compute.com/a2-ultragpu-1g.html
    pub one_gpu_eighty_gb_mem: ProverConfig,
    // https://gcloud-compute.com/a2-ultragpu-2g.html
    pub two_gpu_eighty_gb_mem: ProverConfig,
    // https://gcloud-compute.com/a2-ultragpu-4g.html
    pub four_gpu_eighty_gb_mem: ProverConfig,
}

impl ProverConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }
}
