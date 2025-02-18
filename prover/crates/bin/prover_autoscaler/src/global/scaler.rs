use std::{collections::HashMap, fmt::Debug, hash::Hash, str::FromStr};

use chrono::Utc;
use debug_map_sorted::SortedOutputExt;

use crate::{
    agent::{ScaleDeploymentRequest, ScaleRequest},
    cluster_types::{Cluster, Clusters, PodStatus},
    config::{QueueReportFields, ScalerTarget},
    key::Key,
    metrics::AUTOSCALER_METRICS,
};

const DEFAULT_SPEED: usize = 500;

#[derive(Debug, Eq, Hash, PartialEq)]
pub struct PoolKey<K: Eq + Hash + Copy> {
    pub cluster: String,
    pub key: K,
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Pool<K: Eq + Hash + Copy> {
    name: String,
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

pub struct Scaler<K> {
    pub queue_report_field: QueueReportFields,
    pub deployment: String,
    /// Cluster usage priority when there is no other strong signal. Smaller value is used first.
    cluster_priorities: HashMap<String, u32>,
    apply_min_to_namespace: Option<String>,
    min_replicas: usize,
    max_replicas: HashMap<String, HashMap<K, usize>>,
    // TODO Add default speed for default K
    speed: HashMap<K, usize>,
    long_pending_duration: chrono::Duration,
}

impl<K: Key> Scaler<K> {
    pub fn new(
        config: &ScalerTarget<K>,
        cluster_priorities: HashMap<String, u32>,
        apply_min_to_namespace: Option<String>,
        long_pending_duration: chrono::Duration,
    ) -> Self {
        Self {
            queue_report_field: config.queue_report_field,
            deployment: config.deployment.clone(),
            cluster_priorities,
            apply_min_to_namespace,
            min_replicas: config.min_replicas,
            max_replicas: config.max_replicas.clone(),
            speed: config.speed.clone(),
            long_pending_duration,
        }
    }

    fn convert_to_pool(&self, namespace: &String, cluster: &Cluster) -> Vec<Pool<K>> {
        let Some(namespace_value) = &cluster.namespaces.get(namespace) else {
            // No namespace in config, ignoring.
            return vec![];
        };

        let mut pool_map = HashMap::new(); // <key, Pool>
        for deployment in namespace_value.deployments.keys() {
            // Processing only provers.
            let Some(key) = K::new(&self.deployment, deployment) else {
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
                    .filter(|v| v.time < Utc::now() - chrono::Duration::hours(1)) // TODO Move the duration into config.
                    .count(),
                ..Default::default()
            });

            // Initialize pool only if we have ready deployments.
            e.pods.insert(PodStatus::Running, 0);
        }

        let recent_scale_errors = namespace_value
            .scale_errors
            .iter()
            .filter(|v| v.time < Utc::now() - chrono::Duration::minutes(4)) // TODO Move the duration into config. This should be at least x2 or run interval.
            .count();

        for (pod, pod_value) in namespace_value
            .pods
            .iter()
            .filter(|(p, _)| p.starts_with(&self.deployment))
        {
            let Some(key) = K::new(&self.deployment, pod) else {
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
                if pod_value.changed < Utc::now() - self.long_pending_duration {
                    status = PodStatus::LongPending;
                } else if recent_scale_errors > 0 {
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

    fn sorted_clusters(&self, namespace: &String, clusters: &Clusters) -> Vec<Pool<K>> {
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
                    self.cluster_priorities
                        .get(&a.name)
                        .unwrap_or(&u32::MAX)
                        .cmp(self.cluster_priorities.get(&b.name).unwrap_or(&u32::MAX)),
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

    pub fn run(
        &self,
        namespace: &String,
        queue: u64,
        clusters: &Clusters,
    ) -> HashMap<PoolKey<K>, usize> {
        let sorted_clusters = self.sorted_clusters(namespace, clusters);
        tracing::debug!(
            "Sorted clusters for namespace {}: {:?}",
            namespace,
            &sorted_clusters
        );

        // Increase queue size, if it's too small, to make sure that required min_provers are
        // running.
        let queue: usize = if self.apply_min_to_namespace.as_deref() == Some(namespace.as_str()) {
            self.normalize_queue(K::default(), queue as usize)
                .max(self.pods_to_speed(K::default(), self.min_replicas))
        } else {
            queue as usize
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
                total -= excess as i64 * self.speed(cluster.key) as i64;
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
        namespace: &str,
        pods: HashMap<PoolKey<K>, usize>,
        clusters: &Clusters,
        requests: &mut HashMap<String, ScaleRequest>,
    ) {
        pods.into_iter()
            .for_each(|(PoolKey { cluster, key }, replicas)| {
                let deployment_name = key.to_deployment(&self.deployment);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cluster_types::{Deployment, Namespace, Pod, ScaleEvent},
        key::{Gpu, GpuKey},
    };

    #[tracing_test::traced_test]
    #[test]
    fn test_run() {
        let scaler = Scaler::new(
            &ScalerTarget::<GpuKey> {
                queue_report_field: QueueReportFields::prover_jobs,
                deployment: "circuit-prover-gpu".into(),
                min_replicas: 2,
                max_replicas: [
                    ("foo".into(), [(GpuKey(Gpu::L4), 100)].into()),
                    ("bar".into(), [(GpuKey(Gpu::L4), 100)].into()),
                ]
                .into(),
                ..Default::default()
            },
            [("foo".into(), 0), ("bar".into(), 10)].into(),
            Some("prover-other".into()),
            chrono::Duration::seconds(600),
        );

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
    fn test_run_min_provers() {
        let scaler = Scaler::new(
            &ScalerTarget::<GpuKey> {
                queue_report_field: QueueReportFields::prover_jobs,
                deployment: "circuit-prover-gpu".into(),
                min_replicas: 2,
                max_replicas: [
                    ("foo".into(), [(GpuKey(Gpu::L4), 100)].into()),
                    ("bar".into(), [(GpuKey(Gpu::L4), 100)].into()),
                ]
                .into(),
                ..Default::default()
            },
            [("foo".into(), 0), ("bar".into(), 10)].into(),
            Some("prover".into()),
            chrono::Duration::seconds(600),
        );

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
    fn test_run_need_move() {
        let scaler = Scaler::new(
            &ScalerTarget::<GpuKey> {
                queue_report_field: QueueReportFields::prover_jobs,
                deployment: "circuit-prover-gpu".into(),
                min_replicas: 2,
                max_replicas: [
                    ("foo".into(), [(GpuKey(Gpu::L4), 100)].into()),
                    ("bar".into(), [(GpuKey(Gpu::L4), 100)].into()),
                ]
                .into(),
                ..Default::default()
            },
            [("foo".into(), 0), ("bar".into(), 10)].into(),
            Some("prover".into()),
            chrono::Duration::seconds(600),
        );

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
}
