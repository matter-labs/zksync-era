use zksync_config::configs::{
    BasicWitnessInputProducerConfig, ExperimentalVmConfig, ProtectiveReadsWriterConfig,
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

impl FromEnv for ExperimentalVmConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            playground: envy_load("experimental_vm.playground", "EXPERIMENTAL_VM_PLAYGROUND_")?,
            ..envy_load("experimental_vm", "EXPERIMENTAL_VM_")?
        })
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
    fn experimental_vm_config_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            EXPERIMENTAL_VM_STATE_KEEPER_FAST_VM_MODE=new
            EXPERIMENTAL_VM_PLAYGROUND_FAST_VM_MODE=shadow
            EXPERIMENTAL_VM_PLAYGROUND_DB_PATH=/db/vm_playground
            EXPERIMENTAL_VM_PLAYGROUND_FIRST_PROCESSED_BATCH=123
            EXPERIMENTAL_VM_PLAYGROUND_RESET=true
        "#;
        lock.set_env(config);

        let config = ExperimentalVmConfig::from_env().unwrap();
        assert_eq!(config.state_keeper_fast_vm_mode, FastVmMode::New);
        assert_eq!(config.playground.fast_vm_mode, FastVmMode::Shadow);
        assert_eq!(config.playground.db_path.unwrap(), "/db/vm_playground");
        assert_eq!(config.playground.first_processed_batch, L1BatchNumber(123));
        assert!(config.playground.reset);

        lock.remove_env(&["EXPERIMENTAL_VM_PLAYGROUND_RESET"]);
        let config = ExperimentalVmConfig::from_env().unwrap();
        assert!(!config.playground.reset);

        lock.remove_env(&["EXPERIMENTAL_VM_PLAYGROUND_FIRST_PROCESSED_BATCH"]);
        let config = ExperimentalVmConfig::from_env().unwrap();
        assert_eq!(config.playground.first_processed_batch, L1BatchNumber(0));

        lock.remove_env(&["EXPERIMENTAL_VM_PLAYGROUND_FAST_VM_MODE"]);
        let config = ExperimentalVmConfig::from_env().unwrap();
        assert_eq!(config.playground.fast_vm_mode, FastVmMode::Old);

        lock.remove_env(&["EXPERIMENTAL_VM_PLAYGROUND_DB_PATH"]);
        let config = ExperimentalVmConfig::from_env().unwrap();
        assert!(config.playground.db_path.is_none());
    }
}
