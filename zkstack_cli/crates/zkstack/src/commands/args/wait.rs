use std::{fmt, future::Future, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::time::MissedTickBehavior;
use zkstack_cli_common::logger;

use crate::messages::{
    msg_wait_non_successful_response, msg_wait_not_healthy, msg_wait_starting_polling,
    msg_wait_timeout, MSG_WAIT_POLL_INTERVAL_HELP, MSG_WAIT_TIMEOUT_HELP,
};

#[derive(Debug, Clone, Copy)]
enum PolledComponent {
    Prometheus,
    HealthCheck,
}

impl fmt::Display for PolledComponent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Prometheus => "Prometheus",
            Self::HealthCheck => "health check",
        })
    }
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct WaitArgs {
    #[arg(long, short = 't', value_name = "SECONDS", help = MSG_WAIT_TIMEOUT_HELP, default_value_t = 60)]
    timeout: u64,
    #[arg(long, value_name = "MILLIS", help = MSG_WAIT_POLL_INTERVAL_HELP, default_value_t = 100)]
    poll_interval: u64,
}

impl WaitArgs {
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval)
    }

    pub async fn poll_prometheus(&self, port: u16, verbose: bool) -> anyhow::Result<()> {
        let component = PolledComponent::Prometheus;
        let url = format!("http://127.0.0.1:{port}/metrics");
        self.poll_with_timeout(component, self.poll_inner(component, &url, verbose))
            .await
    }

    pub async fn poll_health_check(&self, url: &str, verbose: bool) -> anyhow::Result<()> {
        let component = PolledComponent::HealthCheck;
        self.poll_with_timeout(component, self.poll_inner(component, url, verbose))
            .await
    }

    pub async fn poll_with_timeout(
        &self,
        component: impl fmt::Display,
        action: impl Future<Output = anyhow::Result<()>>,
    ) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(60), action)
            .await
            .map_err(|_| anyhow::Error::msg(msg_wait_timeout(&component)))?
    }

    async fn poll_inner(
        &self,
        component: PolledComponent,
        url: &str,
        verbose: bool,
    ) -> anyhow::Result<()> {
        let poll_interval = Duration::from_millis(self.poll_interval);
        let mut interval = tokio::time::interval(poll_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        if verbose {
            logger::debug(msg_wait_starting_polling(&component, url, poll_interval));
        }

        let client = reqwest::Client::builder()
            .connect_timeout(poll_interval)
            .build()
            .context("failed to build reqwest::Client")?;

        loop {
            interval.tick().await;

            let response = match client.get(url).send().await {
                Ok(response) => response,
                Err(err) => {
                    if verbose {
                        logger::debug(format!(
                            "Failed to send request to {}: {}",
                            component, err
                        ));
                    }
                    continue;
                }
            };

            match component {
                PolledComponent::Prometheus => {
                    response
                        .error_for_status()
                        .with_context(|| msg_wait_non_successful_response(&component))?;
                    return Ok(());
                }
                PolledComponent::HealthCheck => {
                    if response.status().is_success() {
                        return Ok(());
                    }

                    if response.status() == StatusCode::SERVICE_UNAVAILABLE {
                        if verbose {
                            logger::debug(msg_wait_not_healthy(url));
                        }
                    } else {
                        response
                            .error_for_status()
                            .with_context(|| msg_wait_non_successful_response(&component))?;
                    }
                }
            }
        }
    }
}
