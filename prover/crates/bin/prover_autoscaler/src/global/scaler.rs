use std::{collections::HashMap, str::FromStr};

use chrono::Utc;
use debug_map_sorted::SortedOutputExt;
use regex::Regex;

use super::{queuer, watcher};
use crate::{
    cluster_types::{Cluster, Clusters, PodStatus, GPU},
    metrics::AUTOSCALER_METRICS,
    task_wiring::Task,
};

const DEFAULT_SPEED: u64 = 500;

#[derive(Default, Debug, PartialEq, Eq)]
struct GPUPool {
    name: String,
    gpu: GPU,
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
    gpu: GPU,
}

pub struct Scaler {
    /// namespace to Protocol Version configuration.
    namespaces: HashMap<String, String>,
    watcher: watcher::Watcher,
    queuer: queuer::Queuer,

    /// Which cluster to use first.
    cluster_priorities: HashMap<String, u32>,
    prover_speed: HashMap<GPU, u64>,
    // `gpu` group must be specified.
    prover_deployment_re: Regex,
    prover_pod_re: Regex,
}

impl Scaler {
    pub fn new(watcher: watcher::Watcher, queuer: queuer::Queuer) -> Self {
        Self {
            namespaces: HashMap::from([
                ("prover-red".to_string(), "0.24.2".to_string()),
                ("prover-blue".to_string(), "0.24.1".to_string()),
            ]), // TODO: read from config.
            watcher,
            queuer,

            // TODO: move to config.
            cluster_priorities: HashMap::from([
                ("mainnet2".to_string(), 0),
                ("mainnet1-use4".to_string(), 10),
            ]),
            // TODO: add config.
            prover_speed: HashMap::from([(GPU::L4, 500)]),
            prover_deployment_re: Regex::new(
                r"^prover-gpu-fri-spec-(\d{1,2})?(-(?<gpu>[ltvpa]\d+))?$",
            )
            .unwrap(),
            prover_pod_re: Regex::new(r"^prover-gpu-fri-spec-(\d{1,2})?(-(?<gpu>[ltvpa]\d+))?")
                .unwrap(),
        }
    }

    fn convert2gpu_pool(&self, namespace: &String, cluster: &Cluster) -> Vec<GPUPool> {
        let mut gp_map = HashMap::new(); // <GPU, GPUPool>

        let Some(nv) = &cluster.namespaces.get(namespace) else {
            // No namespace in config, ignoring.
            return vec![];
        };
        for (dn, _dv) in &nv.deployments {
            let Some(caps) = self.prover_deployment_re.captures(dn) else {
                // Not a prover, ignore.
                continue;
            };
            let gpu =
                GPU::from_str(caps.name("gpu").map_or("l4", |m| m.as_str())).unwrap_or_default();
            let e = gp_map.entry(gpu).or_insert(GPUPool {
                name: cluster.name.clone(),
                gpu,
                max_pool_size: 100, // TODO: get from the agent.
                ..Default::default()
            });

            // Initialize pool only if we have ready deployments.
            e.provers.insert(PodStatus::Running, 0);
        }

        for (pn, pv) in &nv.pods {
            let Some(caps) = self.prover_pod_re.captures(pn) else {
                // Not a prover, ignore.
                continue;
            };
            let gpu =
                GPU::from_str(caps.name("gpu").map_or("l4", |m| m.as_str())).unwrap_or_default();
            let e = gp_map.entry(gpu).or_insert(GPUPool {
                name: cluster.name.clone(),
                gpu,
                ..Default::default()
            });
            let mut status = PodStatus::from_str(&pv.status).unwrap_or_default();
            if status == PodStatus::Pending
                && pv.changed < Utc::now() - chrono::Duration::minutes(15)
            // TODO: add duration into config
            {
                status = PodStatus::LongPending;
            }
            println!("pod {}: status: {}, real status: {}", pn, status, pv.status);
            e.provers.entry(status).and_modify(|n| *n += 1).or_insert(1);
        }

        println!("From pods {:?}", gp_map.sorted_debug());

        gp_map.into_values().collect()
    }

    fn sorted_clusters(&self, namespace: &String, clusters: &Clusters) -> Vec<GPUPool> {
        let mut gpu_pools: Vec<GPUPool> = clusters
            .clusters
            .values()
            .flat_map(|c| self.convert2gpu_pool(namespace, c))
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

    fn speed(&self, gpu: GPU) -> u64 {
        self.prover_speed
            .get(&gpu)
            .cloned()
            .unwrap_or(DEFAULT_SPEED)
    }

    fn provers_to_speed(&self, gpu: GPU, n: u32) -> u64 {
        self.speed(gpu) * n as u64
    }

    fn normilize_queue(&self, gpu: GPU, q: u64) -> u64 {
        let speed = self.speed(gpu);
        // Divide and round up if there's any remainder.
        (q + speed - 1) / speed * speed
    }

    fn run(&self, namespace: &String, q: u64, clusters: &Clusters) -> HashMap<GPUPoolKey, u32> {
        let sc = self.sorted_clusters(namespace, clusters);
        dbg!("run, sorted_clusters", namespace, &sc);

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

        // Remove unneeded.
        if (total as u64) > self.normilize_queue(GPU::L4, q) {
            for c in sc.iter().rev() {
                let mut excess_queue = total as u64 - self.normilize_queue(c.gpu, q);
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

        dbg!(total);
        // Add required provers.
        if (total as u64) < q {
            for c in &sc {
                let mut required_queue = q - total as u64;
                let mut required_provers =
                    (self.normilize_queue(c.gpu, required_queue) / self.speed(c.gpu)) as u32;
                let p = provers.entry(c.to_key()).or_default();
                if *p + required_provers > c.max_pool_size {
                    required_provers = c.max_pool_size - *p;
                    required_queue = required_provers as u64 * self.speed(c.gpu);
                }
                *p += required_provers;
                total += required_queue as i64;
            }
        }

        dbg!("run result:", &provers, total);

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
                dbg!(provers);
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
    use crate::cluster_types::{self, Deployment, Namespace, Pod};
    use crate::global::queuer;
    use crate::global::watcher;

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
        let scaler = Scaler::new(watcher, queuer);
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
                gpu: GPU::L4,
            },
            3,
        )]);
        assert!(got == want);
    }
}
