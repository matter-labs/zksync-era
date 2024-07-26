use zksync_config::configs::{BasicWitnessInputProducerConfig, ProtectiveReadsWriterConfig};

use crate::{envy_load, FromEnv};

impl FromEnv for ProtectiveReadsWriterConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("vm_runner.protective_reads", "VM_RUNNER_PROTECTIVE_READS_")
    }
}

impl FromEnv for BasicWitnessInputProducerConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            experimental_vm: envy_load(
                "vm_runner.bwip.experimental_vm",
                "VM_RUNNER_BWIP_EXPERIMENTAL_VM_",
            )?,
            ..envy_load("vm_runner.bwip", "VM_RUNNER_BWIP_")?
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
            VM_RUNNER_BWIP_EXPERIMENTAL_VM_FAST_VM_MODE=shadow
        "#;
        lock.set_env(config);

        let config = BasicWitnessInputProducerConfig::from_env().unwrap();
        assert_eq!(config.db_path, "/db/bwip");
        assert_eq!(config.window_size, 50);
        assert_eq!(config.first_processed_batch, L1BatchNumber(123));
        assert_eq!(
            config.experimental_vm.fast_vm_mode,
            Some(FastVmMode::Shadow)
        );

        lock.remove_env(&["VM_RUNNER_BWIP_EXPERIMENTAL_VM_FAST_VM_MODE"]);

        let config = BasicWitnessInputProducerConfig::from_env().unwrap();
        assert_eq!(config.experimental_vm.fast_vm_mode, None);
    }
}
