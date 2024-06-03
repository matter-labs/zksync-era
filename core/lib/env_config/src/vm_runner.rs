use zksync_config::configs::ProtectiveReadsWriterConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ProtectiveReadsWriterConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("vm_runner.protective_reads", "VM_RUNNER_PROTECTIVE_READS_")
    }
}
