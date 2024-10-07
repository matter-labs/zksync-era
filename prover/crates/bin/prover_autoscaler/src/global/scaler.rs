use std::{collections::HashMap, str::FromStr};

use chrono::Utc;
use debug_map_sorted::SortedOutputExt;
use once_cell::sync::Lazy;
use regex::Regex;
use zksync_config::configs::prover_autoscaler::{Gpu, ProverAutoscalerScalerConfig};

use super::{queuer, watcher};
use crate::{
    cluster_types::{Cluster, Clusters, Pod, PodStatus},
    metrics::AUTOSCALER_METRICS,
    task_wiring::Task,
};

const DEFAULT_SPEED: u32 = 500;

#[derive(Default, Debug, PartialEq, Eq)]
struct GPUPool {
    name: String,
    gpu: Gpu,
    provers: HashMap<PodStatus, u32>, // TODO: consider using i64 everywhere to avoid type casts.
    preemtions: u64,
    max_pool_size: u32,
}

impl GPUPool {
    fn sum_by_pod_status(&self, ps: PodStatus) -> u32 {
        self.provers.get(&ps).cloned().unwrap_or(0)
    }

    fn to_key(&self) -> GPUPoolKey {
        GPUPoolKey {
            cluster: self.name.clone(),
            gpu: self.gpu,
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct GPUPoolKey {
    cluster: String,
    gpu: Gpu,
}

static PROVER_DEPLOYMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^prover-gpu-fri-spec-(\d{1,2})?(-(?<gpu>[ltvpa]\d+))?$").unwrap());
static PROVER_POD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^prover-gpu-fri-spec-(\d{1,2})?(-(?<gpu>[ltvpa]\d+))?").unwrap());

pub struct Scaler {
    /// namespace to Protocol Version configuration.
    namespaces: HashMap<String, String>,
    watcher: watcher::Watcher,
    queuer: queuer::Queuer,

    /// Which cluster to use first.
    cluster_priorities: HashMap<String, u32>,
    prover_speed: HashMap<Gpu, u32>,
    long_pending_duration: chrono::Duration,
}

struct ProverPodGpu<'a> {
    name: &'a str,
    pod: &'a Pod,
    gpu: Gpu,
}

impl<'a> ProverPodGpu<'a> {
    fn new(name: &'a str, pod: &'a Pod) -> Option<ProverPodGpu<'a>> {
        PROVER_POD_RE.captures(name).map(|caps| Self {
            name,
            pod,
            gpu: Gpu::from_str(caps.name("gpu").map_or("l4", |m| m.as_str())).unwrap_or_default(),
        })
    }
}

impl Scaler {
    pub fn new(
        watcher: watcher::Watcher,
        queuer: queuer::Queuer,
        config: ProverAutoscalerScalerConfig,
    ) -> Self {
        Self {
            namespaces: config.protocol_versions,
            watcher,
            queuer,
            cluster_priorities: config.cluster_priorities,
            prover_speed: config.prover_speed,
            long_pending_duration: chrono::Duration::seconds(
                config.long_pending_duration.whole_seconds(),
            ),
        }
    }

    fn convert_to_gpu_pool(&self, namespace: &String, cluster: &Cluster) -> Vec<GPUPool> {
        let mut gp_map = HashMap::new(); // <Gpu, GPUPool>
        let Some(namespace_value) = &cluster.namespaces.get(namespace) else {
            // No namespace in config, ignoring.
            return vec![];
        };

        for caps in namespace_value
            .deployments
            .keys()
            .filter_map(|dn| PROVER_DEPLOYMENT_RE.captures(dn))
        {
            // Processing only provers.
            let gpu =
                Gpu::from_str(caps.name("gpu").map_or("l4", |m| m.as_str())).unwrap_or_default();
            let e = gp_map.entry(gpu).or_insert(GPUPool {
                name: cluster.name.clone(),
                gpu,
                max_pool_size: 100, // TODO: get from the agent.
                ..Default::default()
            });

            // Initialize pool only if we have ready deployments.
            e.provers.insert(PodStatus::Running, 0);
        }

        for ppg in namespace_value
            .pods
            .iter()
            .filter_map(|(pn, pv)| ProverPodGpu::new(pn, pv))
        {
            let e = gp_map.entry(ppg.gpu).or_insert(GPUPool {
                name: cluster.name.clone(),
                gpu: ppg.gpu,
                ..Default::default()
            });
            let mut status = PodStatus::from_str(&ppg.pod.status).unwrap_or_default();
            if status == PodStatus::Pending
                && ppg.pod.changed < Utc::now() - self.long_pending_duration
            {
                status = PodStatus::LongPending;
            }
            tracing::info!(
                "pod {}: status: {}, real status: {}",
                ppg.name,
                status,
                ppg.pod.status
            );
            e.provers.entry(status).and_modify(|n| *n += 1).or_insert(1);
        }

        tracing::info!("From pods {:?}", gp_map.sorted_debug());

        gp_map.into_values().collect()
    }

    fn sorted_clusters(&self, namespace: &String, clusters: &Clusters) -> Vec<GPUPool> {
        let mut gpu_pools: Vec<GPUPool> = clusters
            .clusters
            .values()
            .flat_map(|c| self.convert_to_gpu_pool(namespace, c))
            .collect();

        gpu_pools.sort_by(|a, b| {
            a.gpu
                .cmp(&b.gpu) // Sort by GPU first.
                .then(
                    a.sum_by_pod_status(PodStatus::NeedToMove)
                        .cmp(&b.sum_by_pod_status(PodStatus::NeedToMove)),
                ) // Sort by need to evict.
                .then(
                    a.sum_by_pod_status(PodStatus::LongPending)
                        .cmp(&b.sum_by_pod_status(PodStatus::LongPending)),
                ) // Sort by long Pending pods.
                .then(a.preemtions.cmp(&b.preemtions)) // Sort by preemtions in the cluster.
                .then(
                    self.cluster_priorities
                        .get(&a.name)
                        .unwrap_or(&1000)
                        .cmp(self.cluster_priorities.get(&b.name).unwrap_or(&1000)),
                ) // Sort by priority.
                .then(b.max_pool_size.cmp(&a.max_pool_size)) // Reverse sort by cluster size.
        });

        gpu_pools
    }

    fn speed(&self, gpu: Gpu) -> u64 {
        self.prover_speed
            .get(&gpu)
            .cloned()
            .unwrap_or(DEFAULT_SPEED)
            .into()
    }

    fn provers_to_speed(&self, gpu: Gpu, n: u32) -> u64 {
        self.speed(gpu) * n as u64
    }

    fn normalize_queue(&self, gpu: Gpu, q: u64) -> u64 {
        let speed = self.speed(gpu);
        // Divide and round up if there's any remainder.
        (q + speed - 1) / speed * speed
    }

    fn run(&self, namespace: &String, q: u64, clusters: &Clusters) -> HashMap<GPUPoolKey, u32> {
        let sc = self.sorted_clusters(namespace, clusters);
        tracing::debug!("Sorted clusters for namespace {}: {:?}", namespace, &sc);

        let mut total: i64 = 0;
        let mut provers: HashMap<GPUPoolKey, u32> = HashMap::new();
        for c in &sc {
            for (status, p) in &c.provers {
                match status {
                    PodStatus::Running | PodStatus::Pending => {
                        total += self.provers_to_speed(c.gpu, *p) as i64;
                        provers
                            .entry(c.to_key())
                            .and_modify(|x| *x += p)
                            .or_insert(*p);
                    }
                    _ => (), // Ignore LongPending as not running here.
                }
            }
        }

        // Remove unneeded pods.
        if (total as u64) > self.normalize_queue(Gpu::L4, q) {
            for c in sc.iter().rev() {
                let mut excess_queue = total as u64 - self.normalize_queue(c.gpu, q);
                let mut excess_provers = (excess_queue / self.speed(c.gpu)) as u32;
                let p = provers.entry(c.to_key()).or_default();
                if *p < excess_provers {
                    excess_provers = *p;
                    excess_queue = *p as u64 * self.speed(c.gpu);
                }
                *p -= excess_provers;
                total -= excess_queue as i64;
                if total <= 0 {
                    break;
                };
            }
        }

        // Reduce load in over capacity pools.
        for c in &sc {
            let p = provers.entry(c.to_key()).or_default();
            if c.max_pool_size < *p {
                let excess = *p - c.max_pool_size;
                total -= excess as i64 * self.speed(c.gpu) as i64;
                *p -= excess;
            }
        }

        tracing::debug!("Queue coverd with provers: {}", total);
        // Add required provers.
        if (total as u64) < q {
            for c in &sc {
                let mut required_queue = q - total as u64;
                let mut required_provers =
                    (self.normalize_queue(c.gpu, required_queue) / self.speed(c.gpu)) as u32;
                let p = provers.entry(c.to_key()).or_default();
                if *p + required_provers > c.max_pool_size {
                    required_provers = c.max_pool_size - *p;
                    required_queue = required_provers as u64 * self.speed(c.gpu);
                }
                *p += required_provers;
                total += required_queue as i64;
            }
        }

        tracing::debug!("run result: provers {:?}, total: {}", &provers, total);

        provers
    }
}

#[async_trait::async_trait]
impl Task for Scaler {
    async fn invoke(&self) -> anyhow::Result<()> {
        let queue = self.queuer.get_queue().await.unwrap();

        // TODO: Check that clusters data is ready.
        let clusters = self.watcher.clusters.lock().await;
        for (ns, ppv) in &self.namespaces {
            let q = queue.queue.get(ppv).cloned().unwrap_or(0);
            if q > 0 {
                let provers = self.run(ns, q, &clusters);
                for (k, num) in &provers {
                    AUTOSCALER_METRICS.provers[&(k.cluster.clone(), ns.clone(), k.gpu)]
                        .set(*num as u64);
                }
                // TODO: compare before and desired, send commands [cluster,namespace,deployment] -> provers
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use super::*;
    use crate::{
        cluster_types::{self, Deployment, Namespace, Pod},
        global::{queuer, watcher},
    };

    #[test]
    fn test_run() {
        let watcher = watcher::Watcher {
            cluster_agents: vec![],
            clusters: Arc::new(Mutex::new(cluster_types::Clusters {
                ..Default::default()
            })),
        };
        let queuer = queuer::Queuer {
            prover_job_monitor_url: "".to_string(),
        };
        let scaler = Scaler::new(watcher, queuer, ProverAutoscalerScalerConfig::default());
        let got = scaler.run(
            &"prover".to_string(),
            1499,
            &Clusters {
                clusters: HashMap::from([(
                    "foo".to_string(),
                    Cluster {
                        name: "foo".to_string(),
                        namespaces: HashMap::from([(
                            "prover".to_string(),
                            Namespace {
                                deployments: HashMap::from([(
                                    "prover-gpu-fri-spec-1".to_string(),
                                    Deployment {
                                        ..Default::default()
                                    },
                                )]),
                                pods: HashMap::from([(
                                    "prover-gpu-fri-spec-1-c47644679-x9xqp".to_string(),
                                    Pod {
                                        status: "Running".to_string(),
                                        ..Default::default()
                                    },
                                )]),
                            },
                        )]),
                    },
                )]),
            },
        );
        let want = HashMap::from([(
            GPUPoolKey {
                cluster: "foo".to_string(),
                gpu: Gpu::L4,
            },
            3,
        )]);
        assert!(got == want);
    }
}
