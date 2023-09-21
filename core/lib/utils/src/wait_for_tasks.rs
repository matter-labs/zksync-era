use futures::{future, Future};
use tokio::task::JoinHandle;

use crate::panic_extractor::try_extract_panic_message;

pub async fn wait_for_tasks<Fut>(
    task_futures: Vec<JoinHandle<anyhow::Result<()>>>,
    particular_crypto_alerts: Option<Vec<String>>,
    graceful_shutdown: Option<Fut>,
    tasks_allowed_to_finish: bool,
) where
    Fut: Future<Output = ()>,
{
    match future::select_all(task_futures).await.0 {
        Ok(Ok(())) => {
            if tasks_allowed_to_finish {
                tracing::info!("One of the actors finished its run. Finishing execution.");
            } else {
                let err = "One of the actors finished its run, while it wasn't expected to do it";
                tracing::error!("{err}");
                vlog::capture_message(err, vlog::AlertLevel::Warning);
                if let Some(graceful_shutdown) = graceful_shutdown {
                    graceful_shutdown.await;
                }
            }
        }
        Ok(Err(err)) => {
            let err = format!("One of the tokio actors unexpectedly finished with error: {err}");
            tracing::error!("{err}");
            vlog::capture_message(&err, vlog::AlertLevel::Warning);
            if let Some(graceful_shutdown) = graceful_shutdown {
                graceful_shutdown.await;
            }
        }
        Err(error) => {
            let is_panic = error.is_panic();
            let panic_message = try_extract_panic_message(error);

            tracing::info!(
                "One of the tokio actors unexpectedly finished with error: {panic_message}"
            );

            if is_panic {
                if let Some(particular_alerts) = particular_crypto_alerts {
                    let sporadic_substring_option =
                        particular_alerts
                            .into_iter()
                            .find(|error_message_substring| {
                                panic_message.contains(error_message_substring)
                            });

                    match sporadic_substring_option {
                        Some(_) => {
                            metrics::counter!("server.crypto.panics", 1, "category" => "sporadic", "panic_message" => panic_message.to_string());
                        }
                        None => {
                            metrics::counter!("server.crypto.panics", 1, "category" => "non-sporadic", "panic_message" => panic_message.to_string());
                        }
                    }
                }
            }

            if let Some(graceful_shutdown) = graceful_shutdown {
                graceful_shutdown.await;
            }
        }
    }
}
