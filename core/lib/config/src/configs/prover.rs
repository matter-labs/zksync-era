use std::time::Duration;

// Built-in uses
// External uses
use serde::Deserialize;

// Local uses
use super::envy_load;

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

impl ProverConfigs {
    pub fn from_env() -> Self {
        Self {
            non_gpu: envy_load("non_gpu", "PROVER_NON_GPU_"),
            two_gpu_forty_gb_mem: envy_load("two_gpu_forty_gb_mem", "PROVER_TWO_GPU_FORTY_GB_MEM_"),
            one_gpu_eighty_gb_mem: envy_load(
                "one_gpu_eighty_gb_mem",
                "PROVER_ONE_GPU_EIGHTY_GB_MEM_",
            ),
            two_gpu_eighty_gb_mem: envy_load(
                "two_gpu_eighty_gb_mem",
                "PROVER_TWO_GPU_EIGHTY_GB_MEM_",
            ),
            four_gpu_eighty_gb_mem: envy_load(
                "four_gpu_eighty_gb_mem",
                "PROVER_FOUR_GPU_EIGHTY_GB_MEM_",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ProverConfigs {
        ProverConfigs {
            non_gpu: ProverConfig {
                prometheus_port: 3313,
                initial_setup_key_path: "key".to_owned(),
                key_download_url: "value".to_owned(),
                generation_timeout_in_secs: 2700u16,
                number_of_threads: 2,
                max_attempts: 4,
                polling_duration_in_millis: 5,
                setup_keys_path: "/usr/src/setup-keys".to_string(),
                specialized_prover_group_id: 0,
                number_of_setup_slots: 2,
                assembly_receiver_port: 17791,
                assembly_receiver_poll_time_in_millis: 250,
                assembly_queue_capacity: 5,
            },
            two_gpu_forty_gb_mem: ProverConfig {
                prometheus_port: 3313,
                initial_setup_key_path: "key".to_owned(),
                key_download_url: "value".to_owned(),
                generation_timeout_in_secs: 2700u16,
                number_of_threads: 2,
                max_attempts: 4,
                polling_duration_in_millis: 5,
                setup_keys_path: "/usr/src/setup-keys".to_string(),
                specialized_prover_group_id: 1,
                number_of_setup_slots: 5,
                assembly_receiver_port: 17791,
                assembly_receiver_poll_time_in_millis: 250,
                assembly_queue_capacity: 5,
            },
            one_gpu_eighty_gb_mem: ProverConfig {
                prometheus_port: 3313,
                initial_setup_key_path: "key".to_owned(),
                key_download_url: "value".to_owned(),
                generation_timeout_in_secs: 2700u16,
                number_of_threads: 4,
                max_attempts: 4,
                polling_duration_in_millis: 5,
                setup_keys_path: "/usr/src/setup-keys".to_string(),
                specialized_prover_group_id: 2,
                number_of_setup_slots: 5,
                assembly_receiver_port: 17791,
                assembly_receiver_poll_time_in_millis: 250,
                assembly_queue_capacity: 5,
            },
            two_gpu_eighty_gb_mem: ProverConfig {
                prometheus_port: 3313,
                initial_setup_key_path: "key".to_owned(),
                key_download_url: "value".to_owned(),
                generation_timeout_in_secs: 2700u16,
                number_of_threads: 9,
                max_attempts: 4,
                polling_duration_in_millis: 5,
                setup_keys_path: "/usr/src/setup-keys".to_string(),
                specialized_prover_group_id: 3,
                number_of_setup_slots: 9,
                assembly_receiver_port: 17791,
                assembly_receiver_poll_time_in_millis: 250,
                assembly_queue_capacity: 5,
            },
            four_gpu_eighty_gb_mem: ProverConfig {
                prometheus_port: 3313,
                initial_setup_key_path: "key".to_owned(),
                key_download_url: "value".to_owned(),
                generation_timeout_in_secs: 2700u16,
                number_of_threads: 18,
                max_attempts: 4,
                polling_duration_in_millis: 5,
                setup_keys_path: "/usr/src/setup-keys".to_string(),
                specialized_prover_group_id: 4,
                number_of_setup_slots: 18,
                assembly_receiver_port: 17791,
                assembly_receiver_poll_time_in_millis: 250,
                assembly_queue_capacity: 5,
            },
        }
    }

    const CONFIG: &str = r#"
        PROVER_NON_GPU_PROMETHEUS_PORT="3313"
        PROVER_NON_GPU_INITIAL_SETUP_KEY_PATH="key"
        PROVER_NON_GPU_KEY_DOWNLOAD_URL="value"
        PROVER_NON_GPU_GENERATION_TIMEOUT_IN_SECS=2700
        PROVER_NON_GPU_NUMBER_OF_THREADS="2"
        PROVER_NON_GPU_MAX_ATTEMPTS="4"
        PROVER_NON_GPU_POLLING_DURATION_IN_MILLIS=5
        PROVER_NON_GPU_SETUP_KEYS_PATH="/usr/src/setup-keys"
        PROVER_NON_GPU_NUMBER_OF_SETUP_SLOTS=2
        PROVER_NON_GPU_ASSEMBLY_RECEIVER_PORT=17791
        PROVER_NON_GPU_ASSEMBLY_RECEIVER_POLL_TIME_IN_MILLIS=250
        PROVER_NON_GPU_ASSEMBLY_QUEUE_CAPACITY=5
        PROVER_NON_GPU_SPECIALIZED_PROVER_GROUP_ID=0

        PROVER_TWO_GPU_FORTY_GB_MEM_PROMETHEUS_PORT="3313"
        PROVER_TWO_GPU_FORTY_GB_MEM_INITIAL_SETUP_KEY_PATH="key"
        PROVER_TWO_GPU_FORTY_GB_MEM_KEY_DOWNLOAD_URL="value"
        PROVER_TWO_GPU_FORTY_GB_MEM_GENERATION_TIMEOUT_IN_SECS=2700
        PROVER_TWO_GPU_FORTY_GB_MEM_NUMBER_OF_THREADS="2"
        PROVER_TWO_GPU_FORTY_GB_MEM_MAX_ATTEMPTS="4"
        PROVER_TWO_GPU_FORTY_GB_MEM_POLLING_DURATION_IN_MILLIS=5
        PROVER_TWO_GPU_FORTY_GB_MEM_SETUP_KEYS_PATH="/usr/src/setup-keys"
        PROVER_TWO_GPU_FORTY_GB_MEM_NUMBER_OF_SETUP_SLOTS=5
        PROVER_TWO_GPU_FORTY_GB_MEM_ASSEMBLY_RECEIVER_PORT=17791
        PROVER_TWO_GPU_FORTY_GB_MEM_ASSEMBLY_RECEIVER_POLL_TIME_IN_MILLIS=250
        PROVER_TWO_GPU_FORTY_GB_MEM_ASSEMBLY_QUEUE_CAPACITY=5
        PROVER_TWO_GPU_FORTY_GB_MEM_SPECIALIZED_PROVER_GROUP_ID=1

        PROVER_ONE_GPU_EIGHTY_GB_MEM_PROMETHEUS_PORT="3313"
        PROVER_ONE_GPU_EIGHTY_GB_MEM_INITIAL_SETUP_KEY_PATH="key"
        PROVER_ONE_GPU_EIGHTY_GB_MEM_KEY_DOWNLOAD_URL="value"
        PROVER_ONE_GPU_EIGHTY_GB_MEM_GENERATION_TIMEOUT_IN_SECS=2700
        PROVER_ONE_GPU_EIGHTY_GB_MEM_NUMBER_OF_THREADS="4"
        PROVER_ONE_GPU_EIGHTY_GB_MEM_MAX_ATTEMPTS="4"
        PROVER_ONE_GPU_EIGHTY_GB_MEM_POLLING_DURATION_IN_MILLIS=5
        PROVER_ONE_GPU_EIGHTY_GB_MEM_SETUP_KEYS_PATH="/usr/src/setup-keys"
        PROVER_ONE_GPU_EIGHTY_GB_MEM_NUMBER_OF_SETUP_SLOTS=5
        PROVER_ONE_GPU_EIGHTY_GB_MEM_ASSEMBLY_RECEIVER_PORT=17791
        PROVER_ONE_GPU_EIGHTY_GB_MEM_ASSEMBLY_RECEIVER_POLL_TIME_IN_MILLIS=250
        PROVER_ONE_GPU_EIGHTY_GB_MEM_ASSEMBLY_QUEUE_CAPACITY=5
        PROVER_ONE_GPU_EIGHTY_GB_MEM_SPECIALIZED_PROVER_GROUP_ID=2

        PROVER_TWO_GPU_EIGHTY_GB_MEM_PROMETHEUS_PORT="3313"
        PROVER_TWO_GPU_EIGHTY_GB_MEM_INITIAL_SETUP_KEY_PATH="key"
        PROVER_TWO_GPU_EIGHTY_GB_MEM_KEY_DOWNLOAD_URL="value"
        PROVER_TWO_GPU_EIGHTY_GB_MEM_GENERATION_TIMEOUT_IN_SECS=2700
        PROVER_TWO_GPU_EIGHTY_GB_MEM_NUMBER_OF_THREADS="9"
        PROVER_TWO_GPU_EIGHTY_GB_MEM_MAX_ATTEMPTS="4"
        PROVER_TWO_GPU_EIGHTY_GB_MEM_POLLING_DURATION_IN_MILLIS=5
        PROVER_TWO_GPU_EIGHTY_GB_MEM_SETUP_KEYS_PATH="/usr/src/setup-keys"
        PROVER_TWO_GPU_EIGHTY_GB_MEM_NUMBER_OF_SETUP_SLOTS=9
        PROVER_TWO_GPU_EIGHTY_GB_MEM_ASSEMBLY_RECEIVER_PORT=17791
        PROVER_TWO_GPU_EIGHTY_GB_MEM_ASSEMBLY_RECEIVER_POLL_TIME_IN_MILLIS=250
        PROVER_TWO_GPU_EIGHTY_GB_MEM_ASSEMBLY_QUEUE_CAPACITY=5
        PROVER_TWO_GPU_EIGHTY_GB_MEM_SPECIALIZED_PROVER_GROUP_ID=3

        PROVER_FOUR_GPU_EIGHTY_GB_MEM_PROMETHEUS_PORT="3313"
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_INITIAL_SETUP_KEY_PATH="key"
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_KEY_DOWNLOAD_URL="value"
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_GENERATION_TIMEOUT_IN_SECS=2700
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_NUMBER_OF_THREADS="18"
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_MAX_ATTEMPTS="4"
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_POLLING_DURATION_IN_MILLIS=5
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_SETUP_KEYS_PATH="/usr/src/setup-keys"
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_NUMBER_OF_SETUP_SLOTS=18
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_ASSEMBLY_RECEIVER_PORT=17791
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_ASSEMBLY_RECEIVER_POLL_TIME_IN_MILLIS=250
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_ASSEMBLY_QUEUE_CAPACITY=5
        PROVER_FOUR_GPU_EIGHTY_GB_MEM_SPECIALIZED_PROVER_GROUP_ID=4
    "#;

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        lock.set_env(CONFIG);
        let actual = ProverConfigs::from_env();
        assert_eq!(actual, expected_config());
    }
}
