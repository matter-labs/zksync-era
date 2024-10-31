use std::{collections::HashMap, str::FromStr};

use chrono::Utc;
use debug_map_sorted::SortedOutputExt;
use once_cell::sync::Lazy;
use regex::Regex;
use zksync_config::configs::prover_autoscaler::{
    Gpu, ProverAutoscalerScalerConfig, QueueReportFields, ScalerTarget,
};

use super::{queuer, watcher};
use crate::{
    agent::{ScaleDeploymentRequest, ScaleRequest},
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
    scale_errors: usize,
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
    Lazy::new(|| Regex::new(r"^circuit-prover-gpu(-(?<gpu>[ltvpa]\d+))?$").unwrap());
static PROVER_POD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^circuit-prover-gpu(-(?<gpu>[ltvpa]\d+))?").unwrap());

/// gpu_to_prover converts Gpu type to corresponding deployment name.
fn gpu_to_prover(gpu: Gpu) -> String {
    let s = "circuit-prover-gpu";
    match gpu {
        Gpu::Unknown => "".into(),
        Gpu::L4 => s.into(),
        _ => format!("{}-{}", s, gpu.to_string().to_lowercase()),
    }
}

pub struct Scaler {
    /// namespace to Protocol Version configuration.
    namespaces: HashMap<String, String>,
    watcher: watcher::Watcher,
    queuer: queuer::Queuer,

    jobs: Vec<QueueReportFields>,
    prover_scaler: GpuScaler,
    simple_scalers: Vec<SimpleScaler>,
}

pub struct GpuScaler {
    /// Which cluster to use first.
    cluster_priorities: HashMap<String, u32>,
    min_provers: HashMap<String, u32>,
    max_provers: HashMap<String, HashMap<Gpu, u32>>,
    prover_speed: HashMap<Gpu, u32>,
    long_pending_duration: chrono::Duration,
}

pub struct SimpleScaler {
    queue_report_field: QueueReportFields,
    deployment: String,
    /// Which cluster to use first.
    cluster_priorities: HashMap<String, u32>,
    max_replicas: HashMap<String, usize>,
    speed: usize,
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
        config
            .protocol_versions
            .iter()
            .for_each(|(namespace, version)| {
                AUTOSCALER_METRICS.prover_protocol_version[&(namespace.clone(), version.clone())]
                    .set(1);
            });

        let mut simple_scalers = Vec::default();
        let mut jobs = vec![QueueReportFields::prover_jobs];
        for c in &config.scaler_targets {
            jobs.push(c.queue_report_field);
            simple_scalers.push(SimpleScaler::new(
                c,
                config.cluster_priorities.clone(),
                chrono::Duration::seconds(config.long_pending_duration.whole_seconds()),
            ))
        }
        Self {
            namespaces: config.protocol_versions.clone(),
            watcher,
            queuer,
            jobs,
            prover_scaler: GpuScaler::new(config),
            simple_scalers,
        }
    }
}

impl GpuScaler {
    pub fn new(config: ProverAutoscalerScalerConfig) -> Self {
        Self {
            cluster_priorities: config.cluster_priorities,
            min_provers: config.min_provers,
            max_provers: config.max_provers,
            prover_speed: config.prover_speed,
            long_pending_duration: chrono::Duration::seconds(
                config.long_pending_duration.whole_seconds(),
            ),
        }
    }

    /// Converts a single cluster into vec of GPUPools, one for each GPU.
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
                max_pool_size: self
                    .max_provers
                    .get(&cluster.name)
                    .and_then(|inner_map| inner_map.get(&gpu))
                    .copied()
                    .unwrap_or(0),
                scale_errors: namespace_value
                    .scale_errors
                    .iter()
                    .filter(|v| v.time < Utc::now() - chrono::Duration::hours(1)) // TODO Move the duration into config.
                    .count(),
                ..Default::default()
            });

            // Initialize pool only if we have ready deployments.
            e.provers.insert(PodStatus::Running, 0);
        }

        let recent_scale_errors = namespace_value
            .scale_errors
            .iter()
            .filter(|v| v.time < Utc::now() - chrono::Duration::minutes(4)) // TODO Move the duration into config. This should be at least x2 or run interval.
            .count();

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
            if status == PodStatus::Pending {
                if ppg.pod.changed < Utc::now() - self.long_pending_duration {
                    status = PodStatus::LongPending;
                } else if recent_scale_errors > 0 {
                    status = PodStatus::NeedToMove;
                }
            }
            tracing::info!(
                "pod {}: status: {}, real status: {}",
                ppg.name,
                status,
                ppg.pod.status
            );
            e.provers.entry(status).and_modify(|n| *n += 1).or_insert(1);
        }

        tracing::debug!("From pods {:?}", gp_map.sorted_debug());

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
                .then(a.scale_errors.cmp(&b.scale_errors)) // Sort by scale_errors in the cluster.
                .then(
                    self.cluster_priorities
                        .get(&a.name)
                        .unwrap_or(&1000)
                        .cmp(self.cluster_priorities.get(&b.name).unwrap_or(&1000)),
                ) // Sort by priority.
                .then(b.max_pool_size.cmp(&a.max_pool_size)) // Reverse sort by cluster size.
        });

        gpu_pools.iter().for_each(|p| {
            AUTOSCALER_METRICS.scale_errors[&p.name.clone()].set(p.scale_errors as u64);
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

    fn normalize_queue(&self, gpu: Gpu, queue: u64) -> u64 {
        let speed = self.speed(gpu);
        // Divide and round up if there's any remainder.
        (queue + speed - 1) / speed * speed
    }

    fn run(&self, namespace: &String, queue: u64, clusters: &Clusters) -> HashMap<GPUPoolKey, u32> {
        let sc = self.sorted_clusters(namespace, clusters);
        tracing::debug!("Sorted clusters for namespace {}: {:?}", namespace, &sc);

        // Increase queue size, if it's too small, to make sure that required min_provers are
        // running.
        let queue: u64 = self.min_provers.get(namespace).map_or(queue, |min| {
            self.normalize_queue(Gpu::L4, queue)
                .max(self.provers_to_speed(Gpu::L4, *min))
        });

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
        if (total as u64) > self.normalize_queue(Gpu::L4, queue) {
            for c in sc.iter().rev() {
                let mut excess_queue = total as u64 - self.normalize_queue(c.gpu, queue);
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

        tracing::debug!("Queue covered with provers: {}", total);
        // Add required provers.
        if (total as u64) < queue {
            for c in &sc {
                let mut required_queue = queue - total as u64;
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

        tracing::debug!(
            "run result for namespace {}: provers {:?}, total: {}",
            namespace,
            &provers,
            total
        );

        provers
    }

    fn diff(
        namespace: &str,
        provers: HashMap<GPUPoolKey, u32>,
        clusters: &Clusters,
        requests: &mut HashMap<String, ScaleRequest>,
    ) {
        provers
            .into_iter()
            .for_each(|(GPUPoolKey { cluster, gpu }, replicas)| {
                let prover = gpu_to_prover(gpu);
                clusters
                    .clusters
                    .get(&cluster)
                    .and_then(|c| c.namespaces.get(namespace))
                    .and_then(|ns| ns.deployments.get(&prover))
                    .map_or_else(
                        || {
                            tracing::error!(
                                "Wasn't able to find deployment {} in cluster {}, namespace {}",
                                prover,
                                cluster,
                                namespace
                            )
                        },
                        |deployment| {
                            if deployment.desired != replicas as i32 {
                                requests
                                    .entry(cluster.clone())
                                    .or_default()
                                    .deployments
                                    .push(ScaleDeploymentRequest {
                                        namespace: namespace.into(),
                                        name: prover.clone(),
                                        size: replicas as i32,
                                    });
                            }
                        },
                    );
            })
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Pool {
    name: String,
    pods: HashMap<PodStatus, usize>,
    scale_errors: usize,
    max_pool_size: usize,
}

impl Pool {
    fn sum_by_pod_status(&self, ps: PodStatus) -> usize {
        self.pods.get(&ps).cloned().unwrap_or(0)
    }
}

impl SimpleScaler {
    pub fn new(
        config: &ScalerTarget,
        cluster_priorities: HashMap<String, u32>,
        long_pending_duration: chrono::Duration,
    ) -> Self {
        Self {
            queue_report_field: config.queue_report_field,
            deployment: config.deployment.clone(),
            cluster_priorities,
            max_replicas: config.max_replicas.clone(),
            speed: config.speed,
            long_pending_duration,
        }
    }

    fn convert_to_pool(&self, namespace: &String, cluster: &Cluster) -> Option<Pool> {
        let Some(namespace_value) = &cluster.namespaces.get(namespace) else {
            // No namespace in config, ignoring.
            return None;
        };

        // TODO: Check if related deployment exists.
        let mut pool = Pool {
            name: cluster.name.clone(),
            max_pool_size: self.max_replicas.get(&cluster.name).copied().unwrap_or(0),
            scale_errors: namespace_value
                .scale_errors
                .iter()
                .filter(|v| v.time < Utc::now() - chrono::Duration::hours(1)) // TODO Move the duration into config.
                .count(),
            ..Default::default()
        };

        // Initialize pool only if we have ready deployments.
        pool.pods.insert(PodStatus::Running, 0);

        let pod_re = Regex::new(&format!("^{}-", self.deployment)).unwrap();
        for (_, pod) in namespace_value
            .pods
            .iter()
            .filter(|(name, _)| pod_re.is_match(name))
        {
            let mut status = PodStatus::from_str(&pod.status).unwrap_or_default();
            if status == PodStatus::Pending && pod.changed < Utc::now() - self.long_pending_duration
            {
                status = PodStatus::LongPending;
            }
            pool.pods.entry(status).and_modify(|n| *n += 1).or_insert(1);
        }

        tracing::debug!("Pool pods {:?}", pool);

        Some(pool)
    }

    fn sorted_clusters(&self, namespace: &String, clusters: &Clusters) -> Vec<Pool> {
        let mut pools: Vec<Pool> = clusters
            .clusters
            .values()
            .flat_map(|c| self.convert_to_pool(namespace, c))
            .collect();

        pools.sort_by(|a, b| {
            a.sum_by_pod_status(PodStatus::NeedToMove)
                .cmp(&b.sum_by_pod_status(PodStatus::NeedToMove)) // Sort by need to evict.
                .then(
                    a.sum_by_pod_status(PodStatus::LongPending)
                        .cmp(&b.sum_by_pod_status(PodStatus::LongPending)),
                ) // Sort by long Pending pods.
                .then(a.scale_errors.cmp(&b.scale_errors)) // Sort by scale_errors in the cluster.
                .then(
                    self.cluster_priorities
                        .get(&a.name)
                        .unwrap_or(&1000)
                        .cmp(self.cluster_priorities.get(&b.name).unwrap_or(&1000)),
                ) // Sort by priority.
                .then(b.max_pool_size.cmp(&a.max_pool_size)) // Reverse sort by cluster size.
        });

        pools
    }

    fn pods_to_speed(&self, n: usize) -> u64 {
        (self.speed * n) as u64
    }

    fn normalize_queue(&self, queue: u64) -> u64 {
        let speed = self.speed as u64;
        // Divide and round up if there's any remainder.
        (queue + speed - 1) / speed * speed
    }

    fn run(&self, namespace: &String, queue: u64, clusters: &Clusters) -> HashMap<String, usize> {
        let sorted_clusters = self.sorted_clusters(namespace, clusters);
        tracing::debug!(
            "Sorted clusters for namespace {}: {:?}",
            namespace,
            &sorted_clusters
        );

        let mut total: i64 = 0;
        let mut pods: HashMap<String, usize> = HashMap::new();
        for cluster in &sorted_clusters {
            for (status, replicas) in &cluster.pods {
                match status {
                    PodStatus::Running | PodStatus::Pending => {
                        total += self.pods_to_speed(*replicas) as i64;
                        pods.entry(cluster.name.clone())
                            .and_modify(|x| *x += replicas)
                            .or_insert(*replicas);
                    }
                    _ => (), // Ignore LongPending as not running here.
                }
            }
        }

        // Remove unneeded pods.
        if (total as u64) > self.normalize_queue(queue) {
            for cluster in sorted_clusters.iter().rev() {
                let mut excess_queue = total as u64 - self.normalize_queue(queue);
                let mut excess_pods = excess_queue as usize / self.speed;
                let replicas = pods.entry(cluster.name.clone()).or_default();
                if *replicas < excess_pods {
                    excess_pods = *replicas;
                    excess_queue = *replicas as u64 * self.speed as u64;
                }
                *replicas -= excess_pods;
                total -= excess_queue as i64;
                if total <= 0 {
                    break;
                };
            }
        }

        // Reduce load in over capacity pools.
        for cluster in &sorted_clusters {
            let replicas = pods.entry(cluster.name.clone()).or_default();
            if cluster.max_pool_size < *replicas {
                let excess = *replicas - cluster.max_pool_size;
                total -= (excess * self.speed) as i64;
                *replicas -= excess;
            }
        }

        tracing::debug!("Queue covered with provers: {}", total);
        // Add required pods.
        if (total as u64) < queue {
            for cluster in &sorted_clusters {
                let mut required_queue = queue - total as u64;
                let mut required_pods = self.normalize_queue(required_queue) as usize / self.speed;
                let replicas = pods.entry(cluster.name.clone()).or_default();
                if *replicas + required_pods > cluster.max_pool_size {
                    required_pods = cluster.max_pool_size - *replicas;
                    required_queue = (required_pods * self.speed) as u64;
                }
                *replicas += required_pods;
                total += required_queue as i64;
            }
        }

        tracing::debug!(
            "run result for namespace {}: provers {:?}, total: {}",
            namespace,
            &pods,
            total
        );

        pods
    }

    fn diff(
        &self,
        namespace: &str,
        replicas: HashMap<String, usize>,
        clusters: &Clusters,
        requests: &mut HashMap<String, ScaleRequest>,
    ) {
        let deployment_name = self.deployment.clone();
        replicas.into_iter().for_each(|(cluster, replicas)| {
            clusters
                .clusters
                .get(&cluster)
                .and_then(|c| c.namespaces.get(namespace))
                .and_then(|ns| ns.deployments.get(&deployment_name))
                .map_or_else(
                    || {
                        tracing::error!(
                            "Wasn't able to find deployment {} in cluster {}, namespace {}",
                            deployment_name,
                            cluster,
                            namespace
                        )
                    },
                    |deployment| {
                        if deployment.desired != replicas as i32 {
                            requests
                                .entry(cluster.clone())
                                .or_default()
                                .deployments
                                .push(ScaleDeploymentRequest {
                                    namespace: namespace.into(),
                                    name: deployment_name.clone(),
                                    size: replicas as i32,
                                });
                        }
                    },
                );
        })
    }
}

/// is_namespace_running returns true if there are some pods running in it.
fn is_namespace_running(namespace: &str, clusters: &Clusters) -> bool {
    clusters
        .clusters
        .values()
        .flat_map(|v| v.namespaces.iter())
        .filter_map(|(k, v)| if k == namespace { Some(v) } else { None })
        .flat_map(|v| v.deployments.values())
        .map(
            |d| d.running + d.desired, // If there is something running or expected to run, we
                                       // should re-evaluate the namespace.
        )
        .sum::<i32>()
        > 0
}

#[async_trait::async_trait]
impl Task for Scaler {
    async fn invoke(&self) -> anyhow::Result<()> {
        let queue = self.queuer.get_queue(&self.jobs).await.unwrap();

        let mut scale_requests: HashMap<String, ScaleRequest> = HashMap::new();
        {
            let guard = self.watcher.data.lock().await; // Keeping the lock during all calls of run() for
                                                        // consitency.
            if let Err(err) = watcher::check_is_ready(&guard.is_ready) {
                AUTOSCALER_METRICS.clusters_not_ready.inc();
                tracing::warn!("Skipping Scaler run: {}", err);
                return Ok(());
            }

            for (ns, ppv) in &self.namespaces {
                // Prover
                let q = queue
                    .get(&(ppv.to_string(), QueueReportFields::prover_jobs))
                    .cloned()
                    .unwrap_or(0);
                AUTOSCALER_METRICS.queue[&(ns.clone(), "prover".into())].set(q);
                tracing::debug!("Running eval for namespace {ns} and PPV {ppv} found queue {q}");
                if q > 0 || is_namespace_running(ns, &guard.clusters) {
                    let provers = self.prover_scaler.run(ns, q, &guard.clusters);
                    for (k, num) in &provers {
                        AUTOSCALER_METRICS.provers[&(k.cluster.clone(), ns.clone(), k.gpu)]
                            .set(*num as u64);
                    }
                    GpuScaler::diff(ns, provers, &guard.clusters, &mut scale_requests);
                }

                // Simple Scalers.
                for scaler in &self.simple_scalers {
                    let q = queue
                        .get(&(ppv.to_string(), scaler.queue_report_field))
                        .cloned()
                        .unwrap_or(0);
                    AUTOSCALER_METRICS.queue[&(ns.clone(), scaler.deployment.clone())].set(q);
                    tracing::debug!("Running eval for namespace {ns}, PPV {ppv}, simple scaler {} found queue {q}", scaler.deployment);
                    if q > 0 || is_namespace_running(ns, &guard.clusters) {
                        let replicas = scaler.run(ns, q, &guard.clusters);
                        for (k, num) in &replicas {
                            AUTOSCALER_METRICS.jobs
                                [&(scaler.deployment.clone(), k.clone(), ns.clone())]
                                .set(*num as u64);
                        }
                        scaler.diff(ns, replicas, &guard.clusters, &mut scale_requests);
                    }
                }
            }
        } // Unlock self.watcher.data.

        if let Err(err) = self.watcher.send_scale(scale_requests).await {
            tracing::error!("Failed scale request: {}", err);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_types::{Deployment, Namespace, Pod, ScaleEvent};

    #[tracing_test::traced_test]
    #[test]
    fn test_run() {
        let scaler = GpuScaler::new(ProverAutoscalerScalerConfig {
            cluster_priorities: [("foo".into(), 0), ("bar".into(), 10)].into(),
            min_provers: [("prover-other".into(), 2)].into(),
            max_provers: [
                ("foo".into(), [(Gpu::L4, 100)].into()),
                ("bar".into(), [(Gpu::L4, 100)].into()),
            ]
            .into(),
            ..Default::default()
        });

        assert_eq!(
            scaler.run(
                &"prover".into(),
                1499,
                &Clusters {
                    clusters: [(
                        "foo".into(),
                        Cluster {
                            name: "foo".into(),
                            namespaces: [(
                                "prover".into(),
                                Namespace {
                                    deployments: [(
                                        "circuit-prover-gpu".into(),
                                        Deployment::default(),
                                    )]
                                    .into(),
                                    pods: [(
                                        "circuit-prover-gpu-7c5f8fc747-gmtcr".into(),
                                        Pod {
                                            status: "Running".into(),
                                            ..Default::default()
                                        },
                                    )]
                                    .into(),
                                    ..Default::default()
                                },
                            )]
                            .into(),
                        },
                    )]
                    .into(),
                    ..Default::default()
                },
            ),
            [(
                GPUPoolKey {
                    cluster: "foo".into(),
                    gpu: Gpu::L4,
                },
                3,
            )]
            .into(),
            "3 new provers"
        );
        assert_eq!(
            scaler.run(
                &"prover".into(),
                499,
                &Clusters {
                    clusters: [
                        (
                            "foo".into(),
                            Cluster {
                                name: "foo".into(),
                                namespaces: [(
                                    "prover".into(),
                                    Namespace {
                                        deployments: [(
                                            "circuit-prover-gpu".into(),
                                            Deployment::default(),
                                        )]
                                        .into(),
                                        ..Default::default()
                                    },
                                )]
                                .into(),
                            },
                        ),
                        (
                            "bar".into(),
                            Cluster {
                                name: "bar".into(),
                                namespaces: [(
                                    "prover".into(),
                                    Namespace {
                                        deployments: [(
                                            "circuit-prover-gpu".into(),
                                            Deployment {
                                                running: 1,
                                                desired: 1,
                                            },
                                        )]
                                        .into(),
                                        pods: [(
                                            "circuit-prover-gpu-7c5f8fc747-gmtcr".into(),
                                            Pod {
                                                status: "Running".into(),
                                                ..Default::default()
                                            },
                                        )]
                                        .into(),
                                        ..Default::default()
                                    },
                                )]
                                .into(),
                            },
                        )
                    ]
                    .into(),
                    ..Default::default()
                },
            ),
            [
                (
                    GPUPoolKey {
                        cluster: "foo".into(),
                        gpu: Gpu::L4,
                    },
                    0,
                ),
                (
                    GPUPoolKey {
                        cluster: "bar".into(),
                        gpu: Gpu::L4,
                    },
                    1,
                )
            ]
            .into(),
            "Preserve running"
        );
    }

    #[tracing_test::traced_test]
    #[test]
    fn test_run_min_provers() {
        let scaler = GpuScaler::new(ProverAutoscalerScalerConfig {
            cluster_priorities: [("foo".into(), 0), ("bar".into(), 10)].into(),
            min_provers: [("prover".into(), 2)].into(),
            max_provers: [
                ("foo".into(), [(Gpu::L4, 100)].into()),
                ("bar".into(), [(Gpu::L4, 100)].into()),
            ]
            .into(),
            ..Default::default()
        });

        assert_eq!(
            scaler.run(
                &"prover".into(),
                10,
                &Clusters {
                    clusters: [
                        (
                            "foo".into(),
                            Cluster {
                                name: "foo".into(),
                                namespaces: [(
                                    "prover".into(),
                                    Namespace {
                                        deployments: [(
                                            "circuit-prover-gpu".into(),
                                            Deployment::default(),
                                        )]
                                        .into(),
                                        ..Default::default()
                                    },
                                )]
                                .into(),
                            },
                        ),
                        (
                            "bar".into(),
                            Cluster {
                                name: "bar".into(),
                                namespaces: [(
                                    "prover".into(),
                                    Namespace {
                                        deployments: [(
                                            "circuit-prover-gpu".into(),
                                            Deployment::default(),
                                        )]
                                        .into(),
                                        ..Default::default()
                                    },
                                )]
                                .into(),
                            },
                        )
                    ]
                    .into(),
                    ..Default::default()
                },
            ),
            [
                (
                    GPUPoolKey {
                        cluster: "foo".into(),
                        gpu: Gpu::L4,
                    },
                    2,
                ),
                (
                    GPUPoolKey {
                        cluster: "bar".into(),
                        gpu: Gpu::L4,
                    },
                    0,
                )
            ]
            .into(),
            "Min 2 provers, non running"
        );
        assert_eq!(
            scaler.run(
                &"prover".into(),
                0,
                &Clusters {
                    clusters: [
                        (
                            "foo".into(),
                            Cluster {
                                name: "foo".into(),
                                namespaces: [(
                                    "prover".into(),
                                    Namespace {
                                        deployments: [(
                                            "circuit-prover-gpu".into(),
                                            Deployment {
                                                running: 3,
                                                desired: 3,
                                            },
                                        )]
                                        .into(),
                                        pods: [
                                            (
                                                "circuit-prover-gpu-7c5f8fc747-gmtcr".into(),
                                                Pod {
                                                    status: "Running".into(),
                                                    ..Default::default()
                                                },
                                            ),
                                            (
                                                "circuit-prover-gpu-7c5f8fc747-gmtc2".into(),
                                                Pod {
                                                    status: "Running".into(),
                                                    ..Default::default()
                                                },
                                            ),
                                            (
                                                "circuit-prover-gpu-7c5f8fc747-gmtc3".into(),
                                                Pod {
                                                    status: "Running".into(),
                                                    ..Default::default()
                                                },
                                            )
                                        ]
                                        .into(),
                                        ..Default::default()
                                    },
                                )]
                                .into(),
                            },
                        ),
                        (
                            "bar".into(),
                            Cluster {
                                name: "bar".into(),
                                namespaces: [(
                                    "prover".into(),
                                    Namespace {
                                        deployments: [(
                                            "circuit-prover-gpu".into(),
                                            Deployment {
                                                running: 2,
                                                desired: 2,
                                            },
                                        )]
                                        .into(),
                                        pods: [
                                            (
                                                "circuit-prover-gpu-7c5f8fc747-gmtcr".into(),
                                                Pod {
                                                    status: "Running".into(),
                                                    ..Default::default()
                                                },
                                            ),
                                            (
                                                "circuit-prover-gpu-7c5f8fc747-gmtc2".into(),
                                                Pod {
                                                    status: "Running".into(),
                                                    ..Default::default()
                                                },
                                            )
                                        ]
                                        .into(),
                                        ..Default::default()
                                    },
                                )]
                                .into(),
                            },
                        )
                    ]
                    .into(),
                    ..Default::default()
                },
            ),
            [
                (
                    GPUPoolKey {
                        cluster: "foo".into(),
                        gpu: Gpu::L4,
                    },
                    2,
                ),
                (
                    GPUPoolKey {
                        cluster: "bar".into(),
                        gpu: Gpu::L4,
                    },
                    0,
                )
            ]
            .into(),
            "Min 2 provers, 5 running"
        );
    }

    #[tracing_test::traced_test]
    #[test]
    fn test_run_need_move() {
        let scaler = GpuScaler::new(ProverAutoscalerScalerConfig {
            cluster_priorities: [("foo".into(), 0), ("bar".into(), 10)].into(),
            min_provers: [("prover".into(), 2)].into(),
            max_provers: [
                ("foo".into(), [(Gpu::L4, 100)].into()),
                ("bar".into(), [(Gpu::L4, 100)].into()),
            ]
            .into(),
            long_pending_duration: ProverAutoscalerScalerConfig::default_long_pending_duration(),
            ..Default::default()
        });

        assert_eq!(
            scaler.run(
                &"prover".into(),
                1400,
                &Clusters {
                    clusters: [
                        (
                            "foo".into(),
                            Cluster {
                                name: "foo".into(),
                                namespaces: [(
                                    "prover".into(),
                                    Namespace {
                                        deployments: [(
                                            "circuit-prover-gpu".into(),
                                            Deployment {
                                                running: 3,
                                                desired: 3,
                                            },
                                        )]
                                        .into(),
                                        pods: [
                                            (
                                                "circuit-prover-gpu-7c5f8fc747-gmtcr".into(),
                                                Pod {
                                                    status: "Running".into(),
                                                    changed: Utc::now(),
                                                    ..Default::default()
                                                },
                                            ),
                                            (
                                                "circuit-prover-gpu-7c5f8fc747-gmtc2".into(),
                                                Pod {
                                                    status: "Pending".into(),
                                                    changed: Utc::now(),
                                                    ..Default::default()
                                                },
                                            ),
                                            (
                                                "circuit-prover-gpu-7c5f8fc747-gmtc3".into(),
                                                Pod {
                                                    status: "Running".into(),
                                                    changed: Utc::now(),
                                                    ..Default::default()
                                                },
                                            )
                                        ]
                                        .into(),
                                        scale_errors: vec![ScaleEvent {
                                            name: "circuit-prover-gpu-7c5f8fc747-gmtc2.123456"
                                                .into(),
                                            time: Utc::now() - chrono::Duration::hours(1)
                                        }],
                                    },
                                )]
                                .into(),
                            },
                        ),
                        (
                            "bar".into(),
                            Cluster {
                                name: "bar".into(),
                                namespaces: [(
                                    "prover".into(),
                                    Namespace {
                                        deployments: [(
                                            "circuit-prover-gpu".into(),
                                            Deployment::default(),
                                        )]
                                        .into(),
                                        ..Default::default()
                                    },
                                )]
                                .into(),
                            },
                        )
                    ]
                    .into(),
                    ..Default::default()
                },
            ),
            [
                (
                    GPUPoolKey {
                        cluster: "foo".into(),
                        gpu: Gpu::L4,
                    },
                    2,
                ),
                (
                    GPUPoolKey {
                        cluster: "bar".into(),
                        gpu: Gpu::L4,
                    },
                    1,
                )
            ]
            .into(),
            "Move 1 prover to bar"
        );
    }
}
