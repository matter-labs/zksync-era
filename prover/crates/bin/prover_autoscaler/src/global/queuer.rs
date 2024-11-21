use std::{collections::HashMap, ops::Deref};

use anyhow::{Context, Ok};
use reqwest::Method;
use zksync_prover_job_monitor::autoscaler_queue_reporter::{QueueReport, VersionedQueueReport};
use zksync_utils::http_with_retries::send_request_with_retries;

use crate::{
    config::QueueReportFields,
    metrics::{AUTOSCALER_METRICS, DEFAULT_ERROR_CODE},
};

const MAX_RETRIES: usize = 5;

pub struct Queue(HashMap<(String, QueueReportFields), u64>);

impl Deref for Queue {
    type Target = HashMap<(String, QueueReportFields), u64>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default)]
pub struct Queuer {
    pub prover_job_monitor_url: String,
}

fn target_to_queue(target: QueueReportFields, report: &QueueReport) -> u64 {
    let res = match target {
        QueueReportFields::basic_witness_jobs => report.basic_witness_jobs.all(),
        QueueReportFields::leaf_witness_jobs => report.leaf_witness_jobs.all(),
        QueueReportFields::node_witness_jobs => report.node_witness_jobs.all(),
        QueueReportFields::recursion_tip_witness_jobs => report.recursion_tip_witness_jobs.all(),
        QueueReportFields::scheduler_witness_jobs => report.scheduler_witness_jobs.all(),
        QueueReportFields::proof_compressor_jobs => report.proof_compressor_jobs.all(),
        QueueReportFields::prover_jobs => report.prover_jobs.all(),
    };
    res as u64
}

impl Queuer {
    pub fn new(pjm_url: String) -> Self {
        Self {
            prover_job_monitor_url: pjm_url,
        }
    }

    /// Requests queue report from prover-job-monitor and parse it into Queue HashMap for provided
    /// list of jobs.
    pub async fn get_queue(&self, jobs: &[QueueReportFields]) -> anyhow::Result<Queue> {
        let url = &self.prover_job_monitor_url;
        let response = send_request_with_retries(url, MAX_RETRIES, Method::GET, None, None).await;
        let response = response.map_err(|err| {
            AUTOSCALER_METRICS.calls[&(url.clone(), DEFAULT_ERROR_CODE)].inc();
            anyhow::anyhow!("Failed fetching queue from URL: {url}: {err:?}")
        })?;

        AUTOSCALER_METRICS.calls[&(url.clone(), response.status().as_u16())].inc();
        let response = response
            .json::<Vec<VersionedQueueReport>>()
            .await
            .context("Failed to read response as json")?;
        Ok(Queue(
            response
                .iter()
                .flat_map(|versioned_report| {
                    jobs.iter().map(move |j| {
                        (
                            (versioned_report.version.to_string(), *j),
                            target_to_queue(*j, &versioned_report.report),
                        )
                    })
                })
                .collect::<HashMap<_, _>>(),
        ))
    }
}
