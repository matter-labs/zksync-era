use std::time::Duration;

use prover_service::Params;

use zksync_config::ProverConfig;

#[derive(Debug)]
pub struct ProverParams {
    number_of_threads: u8,
    polling_duration: Duration,
    number_of_setup_slots: u8,
}

impl ProverParams {
    pub(crate) fn new(config: &ProverConfig) -> Self {
        Self {
            number_of_threads: config.number_of_threads as u8,
            polling_duration: Duration::from_millis(config.polling_duration_in_millis),
            number_of_setup_slots: config.number_of_setup_slots,
        }
    }
}

impl Params for ProverParams {
    fn number_of_parallel_synthesis(&self) -> u8 {
        self.number_of_threads
    }

    fn number_of_setup_slots(&self) -> u8 {
        self.number_of_setup_slots
    }

    fn polling_duration(&self) -> Duration {
        self.polling_duration
    }
}
