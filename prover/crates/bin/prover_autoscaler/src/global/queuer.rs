use std::collections::HashMap;

use anyhow::{Context, Ok};
use reqwest::Method;
use zksync_prover_job_monitor::autoscaler_queue_reporter::{QueueReport, VersionedQueueReport};

use crate::{config::QueueReportFields, http_client::HttpClient};

pub type Queue = HashMap<(String, QueueReportFields), usize>;

#[derive(Default)]
pub struct Queuer {
    http_client: HttpClient,
    pub prover_job_monitor_url: String,
}

fn target_to_queue(target: QueueReportFields, report: &QueueReport) -> usize {
    match target {
        QueueReportFields::basic_witness_jobs => report.basic_witness_jobs.all(),
        QueueReportFields::leaf_witness_jobs => report.leaf_witness_jobs.all(),
        QueueReportFields::node_witness_jobs => report.node_witness_jobs.all(),
        QueueReportFields::recursion_tip_witness_jobs => report.recursion_tip_witness_jobs.all(),
        QueueReportFields::scheduler_witness_jobs => report.scheduler_witness_jobs.all(),
        QueueReportFields::proof_compressor_jobs => report.proof_compressor_jobs.all(),
        QueueReportFields::prover_jobs => report.prover_jobs.all(),
    }
}

impl Queuer {
    pub fn new(http_client: HttpClient, pjm_url: String) -> Self {
        Self {
            http_client,
            prover_job_monitor_url: pjm_url,
        }
    }

    /// Requests queue report from prover-job-monitor and parse it into Queue HashMap for provided
    /// list of jobs.
    pub async fn get_queue(&self, jobs: &[QueueReportFields]) -> anyhow::Result<Queue> {
        let url = &self.prover_job_monitor_url;
        let response = self
            .http_client
            .send_request_with_retries(url, Method::GET, None, None)
            .await;
        let response = response
            .map_err(|err| anyhow::anyhow!("Failed fetching queue from URL: {url}: {err:?}"))?;

        let response = response
            .json::<Vec<VersionedQueueReport>>()
            .await
            .context("Failed to read response as json")?;
        Ok(response
            .iter()
            .flat_map(|versioned_report| {
                jobs.iter().map(move |j| {
                    (
                        (versioned_report.version.to_string(), *j),
                        target_to_queue(*j, &versioned_report.report),
                    )
                })
            })
            .collect::<HashMap<_, _>>())
    }
}
