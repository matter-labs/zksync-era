use crate::house_keeper::periodic_job::PeriodicJob;
use zksync_config::configs::{prover::ProverConfig, witness_generator::WitnessGeneratorConfig};
use zksync_dal::ConnectionPool;

#[derive(Debug)]
pub struct WitnessGeneratorMetricsReporter {
    pub witness_generator_config: WitnessGeneratorConfig,
    pub prover_config: ProverConfig,
}

impl WitnessGeneratorMetricsReporter {
    fn report_metrics(&self, connection_pool: ConnectionPool) {
        let mut conn = connection_pool.access_storage_blocking();
        let last_sealed_l1_batch_number = conn.blocks_dal().get_sealed_block_number();
        let min_unproved_l1_batch_number = conn
            .prover_dal()
            .min_unproved_l1_batch_number(self.prover_config.max_attempts)
            .unwrap_or(last_sealed_l1_batch_number);
        let prover_lag = last_sealed_l1_batch_number.0 - min_unproved_l1_batch_number.0;
        metrics::gauge!("server.prover.lag", prover_lag as f64);
    }
}

impl PeriodicJob for WitnessGeneratorMetricsReporter {
    const SERVICE_NAME: &'static str = "WitnessGeneratorMiscReporter";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        self.report_metrics(connection_pool);
    }
}
