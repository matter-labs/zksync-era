use std::collections::HashMap;

use reqwest::Method;
use zksync_prover_job_monitor::autoscaler_queue_reporter::VersionedQueueReport;
use zksync_utils::http_with_retries::send_request_with_retries;

// TODO use zksync_types::protocol_version::ProtocolSemanticVersion;

// Multi group prover, currently not supported.
// #[derive(Debug)]
// pub struct ProverGroupQueue {
//     queue: HashMap<u8, usize>,
// }

#[derive(Debug)]
pub struct Queue {
    pub queue: HashMap<String, u64>,
}

#[derive(Default)]
pub struct Queuer {
    pub prover_job_monitor_url: String,
}
impl Queuer {
    fn new() -> Self {
        Self {
            prover_job_monitor_url:
                //"http://prover-job-monitor.stage2.svc.cluster.local:3074/queue_report".to_string(),
                "http://10.9.17.87:3074/queue_report".to_string(),
            // TODO: add to config
        }
    }
}

impl Queuer {
    pub async fn get_queue(&self) -> anyhow::Result<Queue> {
        let url = &self.prover_job_monitor_url;
        let response = send_request_with_retries(url, 5, Method::GET, None, None).await;
        let res = response
            .map_err(|err| anyhow::anyhow!("Failed fetching queue from url: {url}: {err:?}"))?
            .json::<Vec<VersionedQueueReport>>()
            .await
            .context("Failed to read response as json");
        Ok(Queue {
            // queue: HashMap::from([("0.24.2".to_string(), 1000)]),
            queue: res
                .iter()
                .map(|x| (x.version.to_string(), x.report.prover_jobs.queued as u64))
                .collect::<HashMap<_, _>>(),
        })
    }
}
