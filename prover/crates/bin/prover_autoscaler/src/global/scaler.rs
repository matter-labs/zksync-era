use std::{collections::HashMap, fmt::Debug, hash::Hash, str::FromStr, sync::Arc};

use chrono::Utc;
use debug_map_sorted::SortedOutputExt;

use crate::{
    agent::{ScaleDeploymentRequest, ScaleRequest},
    cluster_types::{Cluster, ClusterName, Clusters, DeploymentName, NamespaceName, PodStatus},
    config::QueueReportFields,
    key::{Gpu, Key},
    metrics::{JobLabels, AUTOSCALER_METRICS},
};

const DEFAULT_SPEED: usize = 500;

#[derive(Debug, Eq, Hash, PartialEq)]
pub struct PoolKey<K: Eq + Hash + Copy> {
    pub cluster: ClusterName,
    pub key: K,
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Pool<K: Eq + Hash + Copy> {
    name: ClusterName,
    key: K,
    pods: HashMap<PodStatus, usize>, // TODO: consider using i64 everywhere to avoid type casts.
    scale_errors: usize,
    max_pool_size: usize,
}

impl<K: Eq + Hash + Copy> Pool<K> {
    fn sum_by_pod_status(&self, ps: PodStatus) -> usize {
        self.pods.get(&ps).cloned().unwrap_or(0)
    }

    fn to_key(&self) -> PoolKey<K> {
        PoolKey {
            cluster: self.name.clone(),
            key: self.key,
        }
    }
}

#[derive(Debug, Default)]
pub struct ScalerConfig {
    /// Cluster usage priority when there is no other strong signal. Smaller value is used first.
    pub cluster_priorities: HashMap<ClusterName, u32>,
    pub apply_min_to_namespace: Option<NamespaceName>,
    pub long_pending_duration: chrono::Duration,
    pub scale_errors_duration: chrono::Duration,
    pub need_to_move_duration: chrono::Duration,
}

#[derive(Debug)]
pub struct Scaler<K> {
    pub queue_report_field: QueueReportFields,
    pub deployment: DeploymentName,
    min_replicas: usize,
    max_replicas: HashMap<ClusterName, HashMap<K, usize>>,
    // TODO Add default speed for default K
    speed: HashMap<K, usize>,

    config: Arc<ScalerConfig>,
}

impl<K: Key> Scaler<K> {
    pub fn new(
        queue_report_field: QueueReportFields,
        deployment: DeploymentName,
        min_replicas: usize,
        max_replicas: HashMap<ClusterName, HashMap<K, usize>>,
        speed: HashMap<K, usize>,
        config: Arc<ScalerConfig>,
    ) -> Self {
        Self {
            queue_report_field,
            deployment,
            min_replicas,
            max_replicas,
            speed,
            config,
        }
    }

    fn convert_to_pool(&self, namespace: &NamespaceName, cluster: &Cluster) -> Vec<Pool<K>> {
        let Some(namespace_value) = &cluster.namespaces.get(namespace) else {
            // No namespace in config, ignoring.
            return vec![];
        };

        let mut pool_map = HashMap::new(); // <key, Pool>
        for deployment in namespace_value.deployments.keys() {
            // Processing only selected deployment(s).
            let Some(key) = K::new(self.deployment.to_str(), deployment) else {
                continue;
            };
            let e = pool_map.entry(key).or_insert(Pool {
                name: cluster.name.clone(),
                key,
                max_pool_size: self
                    .max_replicas
                    .get(&cluster.name)
                    .and_then(|inner_map| inner_map.get(&key))
                    .copied()
                    .unwrap_or(0),
                scale_errors: namespace_value
                    .scale_errors
                    .iter()
                    .filter(|v| v.time > Utc::now() - self.config.scale_errors_duration)
                    .count(),
                ..Default::default()
            });

            // Initialize pool only if we have ready deployments.
            e.pods.insert(PodStatus::Running, 0);
        }

        let need_to_move_errors = namespace_value
            .scale_errors
            .iter()
            .filter(|v| v.time > Utc::now() - self.config.need_to_move_duration)
            .count();

        for (pod, pod_value) in namespace_value.pods.iter() {
            let Some(key) = K::new(self.deployment.to_str(), &(pod.clone().into())) else {
                continue;
            };
            let pool = pool_map.entry(key).or_insert(Pool {
                // TODO: if the pool entry doesn't exists log an error
                name: cluster.name.clone(),
                key,
                ..Default::default()
            });
            let mut status = PodStatus::from_str(&pod_value.status).unwrap_or_default();
            if status == PodStatus::Pending {
                if pod_value.changed < Utc::now() - self.config.long_pending_duration {
                    status = PodStatus::LongPending;
                } else if need_to_move_errors > 0 {
                    status = PodStatus::NeedToMove;
                }
            }
            tracing::info!(
                "pod {}: status: {}, real status: {}",
                pod,
                status,
                pod_value.status
            );
            pool.pods.entry(status).and_modify(|n| *n += 1).or_insert(1);
        }

        tracing::debug!("From pods {:?}", pool_map.sorted_debug());

        pool_map.into_values().collect()
    }

    fn sorted_clusters(&self, namespace: &NamespaceName, clusters: &Clusters) -> Vec<Pool<K>> {
        let mut pools: Vec<Pool<K>> = clusters
            .clusters
            .values()
            .flat_map(|c| self.convert_to_pool(namespace, c))
            .collect();

        pools.sort_by(|a, b| {
            a.key
                .cmp(&b.key) // Sort by Key first.
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
                    self.config
                        .cluster_priorities
                        .get(&a.name)
                        .unwrap_or(&u32::MAX)
                        .cmp(
                            self.config
                                .cluster_priorities
                                .get(&b.name)
                                .unwrap_or(&u32::MAX),
                        ),
                ) // Sort by priority.
                .then(b.max_pool_size.cmp(&a.max_pool_size)) // Reverse sort by cluster size.
        });

        // TODO move to better place
        pools.iter().for_each(|p| {
            AUTOSCALER_METRICS.scale_errors[&p.name.clone()].set(p.scale_errors as u64);
        });

        pools
    }

    fn speed(&self, key: K) -> usize {
        *self.speed.get(&key).unwrap_or(&DEFAULT_SPEED)
    }

    fn pods_to_speed(&self, key: K, n: usize) -> usize {
        self.speed(key) * n
    }

    fn normalize_queue(&self, key: K, queue: usize) -> usize {
        let speed = self.speed(key);
        // Divide and round up if there's any remainder.
        (queue + speed - 1) / speed * speed
    }

    pub fn calculate(
        &self,
        namespace: &NamespaceName,
        queue: usize,
        clusters: &Clusters,
    ) -> HashMap<PoolKey<K>, usize> {
        let sorted_clusters = self.sorted_clusters(namespace, clusters);
        tracing::debug!(
            "Sorted clusters for namespace {}: {:?}",
            namespace,
            &sorted_clusters
        );

        // Increase queue size, if it's too small, to make sure that required min_replicas are
        // running.
        let queue: usize = if self.config.apply_min_to_namespace == Some(namespace.clone()) {
            self.normalize_queue(K::default(), queue)
                .max(self.pods_to_speed(K::default(), self.min_replicas))
        } else {
            queue
        };

        let mut total: i64 = 0;
        let mut pods: HashMap<PoolKey<K>, usize> = HashMap::new();
        for cluster in &sorted_clusters {
            for (status, replicas) in &cluster.pods {
                match status {
                    PodStatus::Running | PodStatus::Pending => {
                        total += self.pods_to_speed(cluster.key, *replicas) as i64;
                        pods.entry(cluster.to_key())
                            .and_modify(|x| *x += replicas)
                            .or_insert(*replicas);
                    }
                    _ => (), // Ignore LongPending as not running here.
                }
            }
        }

        // Remove unneeded pods.
        // Note: K::default() provides suboptimal result on low load and big difference between
        // speed of different keys. But that very rare case, so can be ignored for now.
        if (total as usize) > self.normalize_queue(K::default(), queue) {
            for cluster in sorted_clusters.iter().rev() {
                let mut excess_queue = total as usize - self.normalize_queue(cluster.key, queue);
                let mut excess_replicas = excess_queue / self.speed(cluster.key);
                let replicas = pods.entry(cluster.to_key()).or_default();
                if *replicas < excess_replicas {
                    excess_replicas = *replicas;
                    excess_queue = *replicas * self.speed(cluster.key);
                }
                *replicas -= excess_replicas;
                total -= excess_queue as i64;
                if total <= 0 {
                    break;
                };
            }
        }

        // Reduce load in over capacity pools.
        for cluster in &sorted_clusters {
            let replicas = pods.entry(cluster.to_key()).or_default();
            if cluster.max_pool_size < *replicas {
                let excess = *replicas - cluster.max_pool_size;
                total -= (excess * self.speed(cluster.key)) as i64;
                *replicas -= excess;
            }
        }

        tracing::debug!("Queue covered with pods: {}", total);
        // Add required pods.
        if (total as usize) < queue {
            for cluster in &sorted_clusters {
                let mut required_queue = queue - total as usize;
                let mut required_replicas =
                    self.normalize_queue(cluster.key, required_queue) / self.speed(cluster.key);
                let replicas = pods.entry(cluster.to_key()).or_default();
                if *replicas + required_replicas > cluster.max_pool_size {
                    required_replicas = cluster.max_pool_size - *replicas;
                    required_queue = required_replicas * self.speed(cluster.key);
                }
                *replicas += required_replicas;
                total += required_queue as i64;
            }
        }

        tracing::debug!(
            "run result for namespace {}: pods {:?}, total: {}",
            namespace,
            &pods,
            total
        );

        pods
    }

    pub fn diff(
        &self,
        namespace: &NamespaceName,
        pods: HashMap<PoolKey<K>, usize>,
        clusters: &Clusters,
        requests: &mut HashMap<ClusterName, ScaleRequest>,
    ) {
        pods.into_iter()
            .for_each(|(PoolKey { cluster, key }, replicas)| {
                let deployment_name = key.to_deployment(self.deployment.to_str());
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
                            if deployment.desired != replicas {
                                requests
                                    .entry(cluster.clone())
                                    .or_default()
                                    .deployments
                                    .push(ScaleDeploymentRequest {
                                        namespace: namespace.clone(),
                                        name: deployment_name.clone(),
                                        size: replicas,
                                    });
                            }
                        },
                    );
            })
    }
}

pub trait ScalerTrait {
    fn deployment(&self) -> DeploymentName;
    fn queue_report_field(&self) -> QueueReportFields;
    fn run(
        &self,
        namespace: &NamespaceName,
        queue: usize,
        clusters: &Clusters,
        requests: &mut HashMap<ClusterName, ScaleRequest>,
    );
}

impl<K: Key> ScalerTrait for Scaler<K> {
    fn deployment(&self) -> DeploymentName {
        self.deployment.clone()
    }
    fn queue_report_field(&self) -> QueueReportFields {
        self.queue_report_field
    }

    fn run(
        &self,
        namespace: &NamespaceName,
        queue: usize,
        clusters: &Clusters,
        requests: &mut HashMap<ClusterName, ScaleRequest>,
    ) {
        let replicas = self.calculate(namespace, queue, clusters);
        for (k, num) in &replicas {
            let labels = JobLabels {
                job: self.deployment.clone(),
                target_cluster: k.cluster.clone(),
                target_namespace: namespace.clone(),
                gpu: match k.key.gpu() {
                    Some(gpu) => gpu,
                    None => Gpu::Unknown,
                },
            };
            AUTOSCALER_METRICS.jobs[&labels].set(*num);

            if self.queue_report_field == QueueReportFields::prover_jobs {
                // TODO: Remove after migration to jobs metric.
                AUTOSCALER_METRICS.provers
                    [&(k.cluster.clone(), namespace.clone(), k.key.gpu().unwrap())]
                    .set(*num);
            }
        }
        self.diff(namespace, replicas, clusters, requests);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cluster_types::{Deployment, Namespace, Pod, ScaleEvent},
        key::{Gpu, GpuKey, NoKey},
    };

    fn scaler_config(apply_min_to_namespace: &str) -> Arc<ScalerConfig> {
        Arc::new(ScalerConfig {
            cluster_priorities: [("foo".into(), 0), ("bar".into(), 10)].into(),
            apply_min_to_namespace: Some(apply_min_to_namespace.into()),
            long_pending_duration: chrono::Duration::seconds(600),
            scale_errors_duration: chrono::Duration::seconds(3600),
            need_to_move_duration: chrono::Duration::seconds(4 * 60),
        })
    }

    #[tracing_test::traced_test]
    #[test]
    fn test_calculate() {
        let scaler = Scaler::<GpuKey>::new(
            QueueReportFields::prover_jobs,
            "circuit-prover-gpu".into(),
            2,
            [
                ("foo".into(), [(GpuKey(Gpu::L4), 100)].into()),
                ("bar".into(), [(GpuKey(Gpu::L4), 100)].into()),
            ]
            .into(),
            [(GpuKey(Gpu::L4), 500), (GpuKey(Gpu::T4), 100)].into(),
            scaler_config("prover-other"),
        );

        assert_eq!(
            scaler.calculate(
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
                PoolKey {
                    cluster: "foo".into(),
                    key: GpuKey(Gpu::L4),
                },
                3,
            )]
            .into(),
            "3 new provers"
        );
        assert_eq!(
            scaler.calculate(
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
                    PoolKey {
                        cluster: "foo".into(),
                        key: GpuKey(Gpu::L4),
                    },
                    0,
                ),
                (
                    PoolKey {
                        cluster: "bar".into(),
                        key: GpuKey(Gpu::L4),
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
    fn test_calculate_min_provers() {
        let scaler = Scaler::new(
            QueueReportFields::prover_jobs,
            "circuit-prover-gpu".into(),
            2,
            [
                ("foo".into(), [(GpuKey(Gpu::L4), 100)].into()),
                ("bar".into(), [(GpuKey(Gpu::L4), 100)].into()),
            ]
            .into(),
            [(GpuKey(Gpu::L4), 500), (GpuKey(Gpu::T4), 100)].into(),
            scaler_config("prover"),
        );

        assert_eq!(
            scaler.calculate(
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
                    PoolKey {
                        cluster: "foo".into(),
                        key: GpuKey(Gpu::L4),
                    },
                    2,
                ),
                (
                    PoolKey {
                        cluster: "bar".into(),
                        key: GpuKey(Gpu::L4),
                    },
                    0,
                )
            ]
            .into(),
            "Min 2 provers, non running"
        );
        assert_eq!(
            scaler.calculate(
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
                    PoolKey {
                        cluster: "foo".into(),
                        key: GpuKey(Gpu::L4),
                    },
                    2,
                ),
                (
                    PoolKey {
                        cluster: "bar".into(),
                        key: GpuKey(Gpu::L4),
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
    fn test_calculate_need_move() {
        let scaler = Scaler::new(
            QueueReportFields::prover_jobs,
            "circuit-prover-gpu".into(),
            2,
            [
                ("foo".into(), [(GpuKey(Gpu::L4), 100)].into()),
                ("bar".into(), [(GpuKey(Gpu::L4), 100)].into()),
            ]
            .into(),
            [(GpuKey(Gpu::L4), 500), (GpuKey(Gpu::T4), 100)].into(),
            scaler_config("prover"),
        );

        assert_eq!(
            scaler.calculate(
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
                                            time: Utc::now() - chrono::Duration::minutes(3)
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
                    PoolKey {
                        cluster: "foo".into(),
                        key: GpuKey(Gpu::L4),
                    },
                    2,
                ),
                (
                    PoolKey {
                        cluster: "bar".into(),
                        key: GpuKey(Gpu::L4),
                    },
                    1,
                )
            ]
            .into(),
            "Move 1 prover to bar"
        );
    }

    #[tracing_test::traced_test]
    #[test]
    fn test_calculate_nokey() {
        let scaler = Scaler::<NoKey>::new(
            QueueReportFields::prover_jobs,
            "some-deployment".into(),
            0,
            [
                ("foo".into(), [(NoKey(), 100)].into()),
                ("bar".into(), [(NoKey(), 100)].into()),
            ]
            .into(),
            [(NoKey(), 10)].into(),
            scaler_config(""),
        );

        assert_eq!(
            scaler.calculate(
                &"prover".into(),
                24,
                &Clusters {
                    clusters: [(
                        "foo".into(),
                        Cluster {
                            name: "foo".into(),
                            namespaces: [(
                                "prover".into(),
                                Namespace {
                                    deployments: [(
                                        "some-deployment".into(),
                                        Deployment::default(),
                                    )]
                                    .into(),
                                    pods: [
                                        (
                                            "some-deployment-7c5f8fc747-gmtcr".into(),
                                            Pod {
                                                status: "Running".into(),
                                                ..Default::default()
                                            },
                                        ),
                                        (
                                            "some-other-deployment-7c5f8fc747-12345".into(),
                                            Pod {
                                                status: "Running".into(),
                                                ..Default::default()
                                            },
                                        ),
                                        (
                                            "some-other-deployment-7c5f8fc747-12346".into(),
                                            Pod {
                                                status: "Running".into(),
                                                ..Default::default()
                                            },
                                        ),
                                        (
                                            "some-other-deployment-7c5f8fc747-12347".into(),
                                            Pod {
                                                status: "Running".into(),
                                                ..Default::default()
                                            },
                                        ),
                                    ]
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
                PoolKey {
                    cluster: "foo".into(),
                    key: NoKey(),
                },
                3,
            )]
            .into(),
            "3 new provers"
        );
        assert_eq!(
            scaler.calculate(
                &"prover".into(),
                9,
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
                                            "some-deployment".into(),
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
                                            "some-deployment".into(),
                                            Deployment {
                                                running: 1,
                                                desired: 1,
                                            },
                                        )]
                                        .into(),
                                        pods: [(
                                            "some-deployment-7c5f8fc747-gmtcr".into(),
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
                    PoolKey {
                        cluster: "foo".into(),
                        key: NoKey(),
                    },
                    0,
                ),
                (
                    PoolKey {
                        cluster: "bar".into(),
                        key: NoKey(),
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
    fn test_convert_to_pool() {
        let scaler = Scaler::new(
            QueueReportFields::prover_jobs,
            "circuit-prover-gpu".into(),
            2,
            [("foo".into(), [(GpuKey(Gpu::L4), 100)].into())].into(),
            [(GpuKey(Gpu::L4), 500)].into(),
            scaler_config("prover"),
        );

        let cluster = &Cluster {
            name: "foo".into(),
            namespaces: [(
                "prover".into(),
                Namespace {
                    deployments: [("circuit-prover-gpu".into(), Deployment::default())].into(),
                    pods: [
                        (
                            "circuit-prover-gpu-7c5f8fc747-gmtcr".into(),
                            Pod {
                                status: "Running".into(),
                                ..Default::default()
                            },
                        ),
                        (
                            "circuit-prover-gpu-7c5f8fc747-12345".into(),
                            Pod {
                                status: "Pending".into(),
                                changed: Utc::now() - chrono::Duration::minutes(15),
                                ..Default::default()
                            },
                        ),
                        (
                            "circuit-prover-gpu-7c5f8fc747-12346".into(),
                            Pod {
                                status: "Pending".into(),
                                changed: Utc::now() - chrono::Duration::minutes(2),
                                ..Default::default()
                            },
                        ),
                    ]
                    .into(),
                    scale_errors: vec![ScaleEvent {
                        name: "".into(),
                        time: Utc::now() - chrono::Duration::minutes(1),
                    }],
                },
            )]
            .into(),
        };
        assert_eq!(
            scaler.convert_to_pool(&"prover".into(), cluster),
            vec![Pool {
                name: "foo".into(),
                key: GpuKey(Gpu::L4),
                pods: [
                    (PodStatus::NeedToMove, 1),
                    (PodStatus::LongPending, 1),
                    (PodStatus::Running, 1)
                ]
                .into(),
                scale_errors: 1,
                max_pool_size: 100,
            }]
        );
    }
}
