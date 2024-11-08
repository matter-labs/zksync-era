use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use tokio::time::MissedTickBehavior;

use crate::messages::{MSG_WAIT_POLL_INTERVAL_HELP, MSG_WAIT_TIMEOUT_HELP};

#[derive(Debug, Parser)]
pub struct WaitArgs {
    #[arg(long, short = 't', value_name = "SECONDS", help = MSG_WAIT_TIMEOUT_HELP)]
    timeout: Option<u64>,
    #[arg(long, value_name = "MILLIS", help = MSG_WAIT_POLL_INTERVAL_HELP, default_value_t = 100)]
    poll_interval: u64,
}

impl WaitArgs {
    pub async fn poll_prometheus(&self, port: u16) -> anyhow::Result<()> {
        match self.timeout {
            None => self.poll_prometheus_inner(port).await,
            Some(timeout) => tokio::time::timeout(
                Duration::from_secs(timeout),
                self.poll_prometheus_inner(port),
            )
            .await
            .map_err(|_| anyhow::anyhow!("timed out connecting to Prometheus at :{port}"))?,
        }
    }

    async fn poll_prometheus_inner(&self, port: u16) -> anyhow::Result<()> {
        let poll_interval = Duration::from_millis(self.poll_interval);
        let mut interval = tokio::time::interval(poll_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let client = reqwest::Client::builder()
            .connect_timeout(poll_interval)
            .build()
            .context("failed to build reqwest::Client")?;
        let url = format!("http://127.0.0.1:{port}/metrics");
        loop {
            interval.tick().await;

            let response = match client.get(&url).send().await {
                Ok(response) => response,
                Err(err) if err.is_connect() || err.is_timeout() => {
                    continue;
                }
                Err(err) => {
                    return Err(anyhow::Error::new(err)
                        .context(format!("failed to connect to Prometheus at `{url}`")))
                }
            };

            response
                .error_for_status()
                .context("non-successful Prometheus response")?;
            return Ok(());
        }
    }
}
