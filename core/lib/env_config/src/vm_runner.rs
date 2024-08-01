use zksync_config::configs::{
    BasicWitnessInputProducerConfig, ExperimentalVmPlaygroundConfig, ProtectiveReadsWriterConfig,
};

use crate::{envy_load, FromEnv};

impl FromEnv for ProtectiveReadsWriterConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("vm_runner.protective_reads", "VM_RUNNER_PROTECTIVE_READS_")
    }
}

impl FromEnv for BasicWitnessInputProducerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("vm_runner.bwip", "VM_RUNNER_BWIP_")
    }
}

impl FromEnv for ExperimentalVmPlaygroundConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("vm_runner.playground", "VM_RUNNER_PLAYGROUND_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::{vm::FastVmMode, L1BatchNumber};

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    #[test]
    fn bwip_config_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            VM_RUNNER_BWIP_DB_PATH=/db/bwip
            VM_RUNNER_BWIP_WINDOW_SIZE=50
            VM_RUNNER_BWIP_FIRST_PROCESSED_BATCH=123
        "#;
        lock.set_env(config);

        let config = BasicWitnessInputProducerConfig::from_env().unwrap();
        assert_eq!(config.db_path, "/db/bwip");
        assert_eq!(config.window_size, 50);
        assert_eq!(config.first_processed_batch, L1BatchNumber(123));
    }

    #[test]
    fn playground_config_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            VM_RUNNER_PLAYGROUND_FAST_VM_MODE=shadow
            VM_RUNNER_PLAYGROUND_DB_PATH=/db/vm_playground
            VM_RUNNER_PLAYGROUND_FIRST_PROCESSED_BATCH=123
            VM_RUNNER_PLAYGROUND_RESET=true
        "#;
        lock.set_env(config);

        let config = ExperimentalVmPlaygroundConfig::from_env().unwrap();
        assert_eq!(config.fast_vm_mode, FastVmMode::Shadow);
        assert_eq!(config.db_path, "/db/vm_playground");
        assert_eq!(config.first_processed_batch, L1BatchNumber(123));
        assert!(config.reset);

        lock.remove_env(&["VM_RUNNER_PLAYGROUND_RESET"]);
        let config = ExperimentalVmPlaygroundConfig::from_env().unwrap();
        assert!(!config.reset);

        lock.remove_env(&["VM_RUNNER_PLAYGROUND_FIRST_PROCESSED_BATCH"]);
        let config = ExperimentalVmPlaygroundConfig::from_env().unwrap();
        assert_eq!(config.first_processed_batch, L1BatchNumber(0));

        lock.remove_env(&["VM_RUNNER_PLAYGROUND_FAST_VM_MODE"]);
        let config = ExperimentalVmPlaygroundConfig::from_env().unwrap();
        assert_eq!(config.fast_vm_mode, FastVmMode::Old);

        lock.remove_env(&["VM_RUNNER_PLAYGROUND_DB_PATH"]);
        let config = ExperimentalVmPlaygroundConfig::from_env().unwrap();
        assert!(!config.db_path.is_empty());
    }
}
