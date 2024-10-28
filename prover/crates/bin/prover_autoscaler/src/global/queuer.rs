use std::collections::HashMap;

use anyhow::{Context, Ok};
use reqwest::Method;
use zksync_config::configs::prover_autoscaler::QueueReportFields;
use zksync_prover_job_monitor::autoscaler_queue_reporter::{QueueReport, VersionedQueueReport};
use zksync_types::prover_dal::JobCountStatistics;
use zksync_utils::http_with_retries::send_request_with_retries;

use crate::metrics::{AUTOSCALER_METRICS, DEFAULT_ERROR_CODE};

const MAX_RETRIES: usize = 5;

pub type Queue = HashMap<(String, QueueReportFields), u64>;

#[derive(Default)]
pub struct Queuer {
    pub prover_job_monitor_url: String,
}

fn sum_queue(jobs: JobCountStatistics) -> u64 {
    (jobs.queued + jobs.in_progress) as u64
}

fn target_to_queue(target: QueueReportFields, report: &QueueReport) -> u64 {
    match target {
        QueueReportFields::basic_witness_jobs => sum_queue(report.basic_witness_jobs),
        QueueReportFields::leaf_witness_jobs => sum_queue(report.leaf_witness_jobs),
        QueueReportFields::node_witness_jobs => sum_queue(report.node_witness_jobs),
        QueueReportFields::recursion_tip_witness_jobs => {
            sum_queue(report.recursion_tip_witness_jobs)
        }
        QueueReportFields::scheduler_witness_jobs => sum_queue(report.scheduler_witness_jobs),
        QueueReportFields::proof_compressor_jobs => sum_queue(report.proof_compressor_jobs),
        QueueReportFields::prover_jobs => sum_queue(report.prover_jobs),
    }
}

impl Queuer {
    pub fn new(pjm_url: String) -> Self {
        Self {
            prover_job_monitor_url: pjm_url,
        }
    }

    pub async fn get_queue(&self, jobs: &[QueueReportFields]) -> anyhow::Result<Queue> {
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
        Ok(response
            .iter()
            .flat_map(|x| {
                jobs.iter().map(move |j| {
                    (
                        (x.version.to_string(), j.clone()),
                        target_to_queue(j.clone(), &x.report),
                    )
                })
            })
            .collect::<HashMap<_, _>>())
    }
}
