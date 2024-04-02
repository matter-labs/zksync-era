#[cfg(feature = "gpu")]
pub mod availability_checker {
    use prover_dal::{ConnectionPool, Prover, ProverDal};
    use zksync_types::prover_dal::{GpuProverInstanceStatus, SocketAddress};

    use crate::metrics::METRICS;

    pub struct AvailabilityChecker {
        address: SocketAddress,
        zone: String,
        polling_interval_secs: u32,
        pool: ConnectionPool<Prover>,
    }

    impl AvailabilityChecker {
        pub fn new(
            address: SocketAddress,
            zone: String,
            polling_interval_secs: u32,
            pool: ConnectionPool<Prover>,
        ) -> Self {
            Self {
                address,
                zone,
                polling_interval_secs,
                pool,
            }
        }

        pub async fn run(
            self,
            stop_receiver: tokio::sync::watch::Receiver<bool>,
        ) -> anyhow::Result<()> {
            while !*stop_receiver.borrow() {
                let status = self
                    .pool
                    .connection()
                    .await
                    .unwrap()
                    .fri_gpu_prover_queue_dal()
                    .get_prover_instance_status(self.address.clone(), self.zone.clone())
                    .await;

                if status.is_none() || status.unwrap() == GpuProverInstanceStatus::Dead {
                    METRICS.zombie_prover_instances_count.inc();
                    tracing::info!(
                        "Prover instance at address {:?}, availability zone {} was found marked as dead while being alive, shutting down",
                        self.address,
                        self.zone
                    );
                    return Ok(());
                }

                tokio::time::sleep(std::time::Duration::from_secs(
                    self.polling_interval_secs as u64,
                ))
                .await;
            }

            tracing::info!("Availability checker was shut down");

            Ok(())
        }
    }
}
