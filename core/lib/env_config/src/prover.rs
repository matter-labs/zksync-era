use zksync_config::ProverConfigs;

use crate::{envy_load, FromEnv};

impl FromEnv for ProverConfigs {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            non_gpu: envy_load("non_gpu", "PROVER_NON_GPU_")?,
            two_gpu_forty_gb_mem: envy_load(
                "two_gpu_forty_gb_mem",
                "PROVER_TWO_GPU_FORTY_GB_MEM_",
            )?,
            one_gpu_eighty_gb_mem: envy_load(
                "one_gpu_eighty_gb_mem",
                "PROVER_ONE_GPU_EIGHTY_GB_MEM_",
            )?,
            two_gpu_eighty_gb_mem: envy_load(
                "two_gpu_eighty_gb_mem",
                "PROVER_TWO_GPU_EIGHTY_GB_MEM_",
            )?,
            four_gpu_eighty_gb_mem: envy_load(
                "four_gpu_eighty_gb_mem",
                "PROVER_FOUR_GPU_EIGHTY_GB_MEM_",
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::ProverConfig;

    use super::*;
    use crate::test_utils::EnvMutex;

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
        let actual = ProverConfigs::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
