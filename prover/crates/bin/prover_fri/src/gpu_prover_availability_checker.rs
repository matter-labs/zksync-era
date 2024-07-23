#[cfg(feature = "gpu")]
pub mod availability_checker {
    use std::{sync::Arc, time::Duration};

    use tokio::sync::Notify;
    use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
    use zksync_prover_fri_utils::region_fetcher::Zone;
    use zksync_types::prover_dal::{GpuProverInstanceStatus, SocketAddress};

    use crate::metrics::{KillingReason, METRICS};

    /// Availability checker is a task that periodically checks the status of the prover instance in the database.
    /// If the prover instance is not found in the database or marked as dead, the availability checker will shut down the prover.
    pub struct AvailabilityChecker {
        address: SocketAddress,
        zone: Zone,
        polling_interval: Duration,
        pool: ConnectionPool<Prover>,
    }

    impl AvailabilityChecker {
        pub fn new(
            address: SocketAddress,
            zone: Zone,
            polling_interval_secs: u32,
            pool: ConnectionPool<Prover>,
        ) -> Self {
            Self {
                address,
                zone,
                polling_interval: Duration::from_secs(polling_interval_secs as u64),
                pool,
            }
        }

        pub async fn run(
            self,
            stop_receiver: tokio::sync::watch::Receiver<bool>,
            init_notifier: Arc<Notify>,
        ) -> anyhow::Result<()> {
            init_notifier.notified().await;

            while !*stop_receiver.borrow() {
                let status = self
                    .pool
                    .connection()
                    .await
                    .unwrap()
                    .fri_gpu_prover_queue_dal()
                    .get_prover_instance_status(self.address.clone(), self.zone.to_string())
                    .await;

                // If the prover instance is not found in the database or marked as dead, we should shut down the prover
                match status {
                    None => {
                        METRICS.zombie_prover_instances_count[&KillingReason::Absent].inc();
                        tracing::info!(
                            "Prover instance at address {:?}, availability zone {} was not found in the database, shutting down",
                            self.address,
                            self.zone
                        );
                        // After returning from the task, it will shut down all the other tasks
                        return Ok(());
                    }
                    Some(GpuProverInstanceStatus::Dead) => {
                        METRICS.zombie_prover_instances_count[&KillingReason::Dead].inc();
                        tracing::info!(
                            "Prover instance at address {:?}, availability zone {} was found marked as dead, shutting down",
                            self.address,
                            self.zone
                        );
                        // After returning from the task, it will shut down all the other tasks
                        return Ok(());
                    }
                    Some(_) => (),
                }

                tokio::time::sleep(self.polling_interval).await;
            }

            tracing::info!("Availability checker was shut down");

            Ok(())
        }
    }
}
