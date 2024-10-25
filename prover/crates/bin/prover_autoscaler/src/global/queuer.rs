use std::collections::HashMap;

use anyhow::{Context, Ok};
use reqwest::Method;
use zksync_prover_job_monitor::autoscaler_queue_reporter::VersionedQueueReport;
use zksync_utils::http_with_retries::send_request_with_retries;

use crate::metrics::{AUTOSCALER_METRICS, DEFAULT_ERROR_CODE};

const MAX_RETRIES: usize = 5;

#[derive(Debug)]
pub struct Queue {
    pub queue: HashMap<String, u64>,
}

#[derive(Default)]
pub struct Queuer {
    pub prover_job_monitor_url: String,
}

impl Queuer {
    pub fn new(pjm_url: String) -> Self {
        Self {
            prover_job_monitor_url: pjm_url,
        }
    }

    pub async fn get_queue(&self) -> anyhow::Result<Queue> {
        let url = &self.prover_job_monitor_url;
        let response = send_request_with_retries(url, MAX_RETRIES, Method::GET, None, None).await;
        let response = response.map_err(|err| {
            AUTOSCALER_METRICS.calls[&(url.clone(), DEFAULT_ERROR_CODE)].inc();
            anyhow::anyhow!("Failed fetching queue from url: {url}: {err:?}")
        })?;

        AUTOSCALER_METRICS.calls[&(url.clone(), response.status().as_u16())].inc();
        let response = response
            .json::<Vec<VersionedQueueReport>>()
            .await
            .context("Failed to read response as json")?;
        Ok(Queue {
            queue: response
                .iter()
                .map(|x| (x.version.to_string(), x.report.prover_jobs.queued as u64))
                .collect::<HashMap<_, _>>(),
        })
    }
}
