//! The planner: a pure function from (config, observations, hysteresis state)
//! to a decision. All policy lives here and nothing here talks to the network,
//! so the whole thing is unit-testable and dry-run is just "skip the actuator".
//!
//! Control law per deployment (drain horizon, not the legacy `queue / speed`):
//!
//! ```text
//! work      = ready + in_progress + reclaimable   (+ pre-warm for snark-only)
//! n_backlog = ceil(work × duration_p95 / drain_window)
//! desired   = clamp(n_backlog, min, max)          (then the group GPU budget)
//! ```
//!
//! Scarce-machine rules:
//! - `spec_replicas` is the capacity already requested: Pending pods are never
//!   re-requested, and each missing replica has one outstanding attempt.
//! - Scale-up moves one replica at a time into the first cluster without
//!   starved Pending pods; when every cluster is starved the planner holds
//!   and reports starvation instead of piling up requests.
//! - Scale-down removes one replica at a time from the last cluster, only
//!   after sustained low demand and only when the fleet is settled (no
//!   Pending or Terminating pods), and the stabilization timer restarts
//!   after every removal.

use std::cmp::Ordering;
use std::time::{Duration, Instant};

use crate::config::{DeploymentConfig, GroupConfig, Mode, Stage};
use crate::queue::GroupQueues;

/// Observed state of one deployment in one cluster.
#[derive(Clone, Debug)]
pub struct ClusterDeployment {
    pub cluster: String,
    /// `spec.replicas` — the capacity already requested from this cluster.
    pub spec_replicas: u32,
    pub running: u32,
    pub pending: u32,
    pub oldest_pending_age: Option<Duration>,
    pub terminating: u32,
}

/// Per-deployment hysteresis state, kept in memory between reconcile ticks.
/// Lost on restart, which is the conservative direction: a fresh process
/// re-confirms demand before scaling.
#[derive(Debug, Default)]
pub struct DeploymentState {
    pub up_streak: u32,
    pub low_demand_since: Option<Instant>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Hold {
        reason: String,
    },
    Scale {
        cluster: String,
        to: u32,
        reason: String,
    },
}

pub struct PlanInput<'a> {
    pub mode: Mode,
    pub deployment: &'a DeploymentConfig,
    pub group: &'a GroupConfig,
    /// `None` when queue data is stale/unavailable — the planner holds.
    pub queues: Option<&'a GroupQueues>,
    /// One entry per cluster, in fill-priority order.
    pub clusters: &'a [ClusterDeployment],
    /// Group GPU budget not yet consumed by higher-priority deployments.
    pub budget_remaining: u32,
    pub now: Instant,
}

pub fn plan(input: &PlanInput<'_>, state: &mut DeploymentState) -> Decision {
    let Some(queues) = input.queues else {
        // Never act on missing data: not up (might be phantom demand), and
        // especially not down (the backlog may be real and invisible).
        state.up_streak = 0;
        state.low_demand_since = None;
        return Decision::Hold {
            reason: "queue report stale or unavailable".into(),
        };
    };

    let cfg = input.deployment;
    let queue = match input.mode.stage() {
        Stage::Fri => &queues.fri,
        Stage::Snark => &queues.snark,
    };

    let mut work = queue.outstanding();
    // Pre-warm: a snark-only pod started now becomes Running after
    // ~startup_p95; count the FRI jobs expected to finish within that window
    // so the wrap doesn't eat a cold start after every FRI proof.
    if input.mode == Mode::SnarkOnly {
        work += expected_fri_completions_during_startup(input.group, queues);
    }

    let n_backlog = ceil_backlog(work, cfg.duration_p95, cfg.drain_window);
    let desired = n_backlog
        .clamp(cfg.min, cfg.max)
        .min(input.budget_remaining);

    let spec: u32 = input.clusters.iter().map(|c| c.spec_replicas).sum();
    let running: u32 = input.clusters.iter().map(|c| c.running).sum();
    let pending: u32 = input.clusters.iter().map(|c| c.pending).sum();
    let terminating: u32 = input.clusters.iter().map(|c| c.terminating).sum();

    match desired.cmp(&spec) {
        Ordering::Greater => scale_up(input, state, queue, work, desired, spec, pending),
        Ordering::Less => scale_down(
            input,
            state,
            work,
            desired,
            spec,
            running,
            pending,
            terminating,
        ),
        Ordering::Equal => {
            state.up_streak = 0;
            state.low_demand_since = None;
            Decision::Hold {
                reason: format!("at target (work={work}, replicas={desired})"),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn scale_up(
    input: &PlanInput<'_>,
    state: &mut DeploymentState,
    queue: &crate::queue::StageQueue,
    work: u64,
    desired: u32,
    spec: u32,
    pending: u32,
) -> Decision {
    let cfg = input.deployment;
    state.low_demand_since = None;
    state.up_streak += 1;
    let slo_breach = queue
        .oldest_ready_age
        .is_some_and(|age| age >= input.group.latency_slo);
    if state.up_streak < cfg.scale_up_confirmations && !slo_breach {
        return Decision::Hold {
            reason: format!(
                "scale-up awaiting confirmation ({}/{}; work={work}, desired={desired}, spec={spec})",
                state.up_streak, cfg.scale_up_confirmations
            ),
        };
    }
    let starved = |c: &&ClusterDeployment| {
        c.oldest_pending_age
            .is_some_and(|age| age >= input.group.startup_p95)
    };
    match input.clusters.iter().find(|c| !starved(c)) {
        Some(target) => {
            state.up_streak = 0;
            Decision::Scale {
                cluster: target.cluster.clone(),
                to: target.spec_replicas + 1,
                reason: format!(
                    "work={work} needs {desired} replicas, have {spec}{}",
                    if slo_breach {
                        " (latency SLO breached)"
                    } else {
                        ""
                    }
                ),
            }
        }
        // No fallback pool exists with a single card class; holding and
        // surfacing starvation is the honest behavior.
        None => Decision::Hold {
            reason: format!(
                "capacity starved: {pending} pod(s) pending beyond startup_p95 in every cluster"
            ),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn scale_down(
    input: &PlanInput<'_>,
    state: &mut DeploymentState,
    work: u64,
    desired: u32,
    spec: u32,
    running: u32,
    pending: u32,
    terminating: u32,
) -> Decision {
    let cfg = input.deployment;
    state.up_streak = 0;
    if pending > 0 || terminating > 0 || running != spec {
        // A churning fleet gives no reliable low-demand signal, and we
        // never overlap a scale-down with an in-flight drain.
        state.low_demand_since = None;
        return Decision::Hold {
            reason: format!(
                "fleet not settled (running={running}, pending={pending}, terminating={terminating}, spec={spec})"
            ),
        };
    }
    let since = *state.low_demand_since.get_or_insert(input.now);
    let sustained = input.now.duration_since(since);
    if sustained < cfg.scale_down_stabilization {
        return Decision::Hold {
            reason: format!(
                "scale-down stabilizing ({}s of {}s; desired={desired}, spec={spec})",
                sustained.as_secs(),
                cfg.scale_down_stabilization.as_secs()
            ),
        };
    }
    let target = input
        .clusters
        .iter()
        .rev()
        .find(|c| c.spec_replicas > 0)
        .expect("spec > desired >= 0, so some cluster has replicas");
    // One at a time: restart the timer so the next removal waits a full
    // stabilization window again.
    state.low_demand_since = None;
    Decision::Scale {
        cluster: target.cluster.clone(),
        to: target.spec_replicas - 1,
        reason: format!(
            "sustained low demand for {}s (work={work}, desired={desired}, spec={spec})",
            sustained.as_secs()
        ),
    }
}

/// `ceil(work × duration / window)` in integer arithmetic.
fn ceil_backlog(work: u64, duration: Duration, window: Duration) -> u32 {
    if work == 0 {
        return 0;
    }
    let num = work as u128 * duration.as_millis().max(1);
    let den = window.as_millis().max(1);
    num.div_ceil(den).min(u32::MAX as u128) as u32
}

/// FRI jobs expected to finish within one machine startup interval, i.e. the
/// in-progress count scaled by `startup_p95 / fri_duration_p95` (capped at 1:
/// a job can finish at most once).
fn expected_fri_completions_during_startup(group: &GroupConfig, queues: &GroupQueues) -> u64 {
    let fri_duration = group
        .deployments
        .get(&Mode::FriOnly)
        .or_else(|| group.deployments.get(&Mode::FriSnark))
        .map(|d| d.duration_p95);
    let Some(fri_duration) = fri_duration else {
        return 0;
    };
    let fraction =
        (group.startup_p95.as_secs_f64() / fri_duration.as_secs_f64().max(f64::EPSILON)).min(1.0);
    (queues.fri.in_progress as f64 * fraction).floor() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::StageQueue;
    use std::collections::BTreeMap;

    fn minutes(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    fn deployment(min: u32, max: u32) -> DeploymentConfig {
        DeploymentConfig {
            name: None,
            duration_p95: minutes(15),
            drain_window: minutes(30),
            min,
            max,
            scale_up_confirmations: 2,
            scale_down_stabilization: minutes(25),
        }
    }

    fn group(deployments: BTreeMap<Mode, DeploymentConfig>) -> GroupConfig {
        GroupConfig {
            compatibility_id: None,
            chains: vec!["http://chain".into()],
            max_gpus: 8,
            startup_p95: minutes(8),
            queue_staleness_limit: minutes(5),
            latency_slo: minutes(240),
            deployments,
        }
    }

    fn fri_queues(ready: u64, in_progress: u64) -> GroupQueues {
        GroupQueues {
            fri: StageQueue {
                ready,
                in_progress,
                reclaimable: 0,
                oldest_ready_age: None,
            },
            snark: StageQueue::default(),
        }
    }

    fn cluster(name: &str, spec: u32, running: u32, pending: u32) -> ClusterDeployment {
        ClusterDeployment {
            cluster: name.into(),
            spec_replicas: spec,
            running,
            pending,
            oldest_pending_age: None,
            terminating: 0,
        }
    }

    fn plan_once(
        mode: Mode,
        group: &GroupConfig,
        queues: Option<&GroupQueues>,
        clusters: &[ClusterDeployment],
        budget: u32,
        state: &mut DeploymentState,
        now: Instant,
    ) -> Decision {
        plan(
            &PlanInput {
                mode,
                deployment: &group.deployments[&mode],
                group,
                queues,
                clusters,
                budget_remaining: budget,
                now,
            },
            state,
        )
    }

    #[test]
    fn backlog_math_matches_drain_horizon() {
        // 4 jobs × 15m / 30m window = 2 replicas.
        assert_eq!(ceil_backlog(4, minutes(15), minutes(30)), 2);
        // Rounds up: 5 jobs → ceil(2.5) = 3.
        assert_eq!(ceil_backlog(5, minutes(15), minutes(30)), 3);
        assert_eq!(ceil_backlog(0, minutes(15), minutes(30)), 0);
    }

    #[test]
    fn scale_up_requires_confirmation_then_fires() {
        let g = group(BTreeMap::from([(Mode::FriSnark, deployment(1, 4))]));
        let queues = fri_queues(4, 0);
        let clusters = [cluster("a", 1, 1, 0)];
        let mut state = DeploymentState::default();
        let now = Instant::now();

        let first = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &clusters,
            8,
            &mut state,
            now,
        );
        assert!(matches!(first, Decision::Hold { .. }), "{first:?}");

        let second = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &clusters,
            8,
            &mut state,
            now,
        );
        assert_eq!(
            second,
            Decision::Scale {
                cluster: "a".into(),
                to: 2,
                reason: "work=4 needs 2 replicas, have 1".into(),
            }
        );
        assert_eq!(state.up_streak, 0, "streak resets after scaling");
    }

    #[test]
    fn slo_breach_bypasses_confirmation() {
        let g = group(BTreeMap::from([(Mode::FriSnark, deployment(1, 4))]));
        let mut queues = fri_queues(4, 0);
        queues.fri.oldest_ready_age = Some(minutes(300));
        let clusters = [cluster("a", 1, 1, 0)];
        let mut state = DeploymentState::default();

        let decision = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &clusters,
            8,
            &mut state,
            Instant::now(),
        );
        assert!(
            matches!(decision, Decision::Scale { to: 2, .. }),
            "{decision:?}"
        );
    }

    #[test]
    fn pending_pods_count_as_requested_capacity() {
        let g = group(BTreeMap::from([(Mode::FriSnark, deployment(1, 4))]));
        let queues = fri_queues(4, 0);
        // spec=2 covers desired=2 even though only 1 pod runs: never
        // re-request the replica that is already Pending.
        let clusters = [cluster("a", 2, 1, 1)];
        let mut state = DeploymentState::default();

        let decision = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &clusters,
            8,
            &mut state,
            Instant::now(),
        );
        assert!(matches!(decision, Decision::Hold { .. }), "{decision:?}");
        assert_eq!(state.up_streak, 0);
    }

    #[test]
    fn starved_clusters_hold_instead_of_requesting_more() {
        let g = group(BTreeMap::from([(Mode::FriSnark, deployment(1, 4))]));
        let queues = fri_queues(8, 0);
        let mut starved = cluster("a", 2, 1, 1);
        starved.oldest_pending_age = Some(minutes(20)); // > startup_p95 = 8m
        let mut state = DeploymentState {
            up_streak: 5,
            low_demand_since: None,
        };

        let decision = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &[starved],
            8,
            &mut state,
            Instant::now(),
        );
        match decision {
            Decision::Hold { reason } => assert!(reason.contains("starved"), "{reason}"),
            other => panic!("expected starvation hold, got {other:?}"),
        }
    }

    #[test]
    fn scale_down_waits_for_stabilization_and_settled_fleet() {
        let g = group(BTreeMap::from([(Mode::FriSnark, deployment(1, 4))]));
        let queues = fri_queues(0, 0); // desired = min = 1
        let mut state = DeploymentState::default();
        let t0 = Instant::now();

        // Unsettled fleet (pending pod): no timer, no scale-down.
        let churning = [cluster("a", 3, 2, 1)];
        let decision = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &churning,
            8,
            &mut state,
            t0,
        );
        assert!(matches!(decision, Decision::Hold { .. }), "{decision:?}");
        assert!(state.low_demand_since.is_none());

        // Settled: the timer starts but hasn't elapsed.
        let settled = [cluster("a", 3, 3, 0)];
        let decision = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &settled,
            8,
            &mut state,
            t0,
        );
        assert!(matches!(decision, Decision::Hold { .. }), "{decision:?}");

        // After the stabilization window: remove exactly one replica.
        let later = t0 + minutes(26);
        let decision = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &settled,
            8,
            &mut state,
            later,
        );
        assert!(
            matches!(decision, Decision::Scale { to: 2, .. }),
            "{decision:?}"
        );
        assert!(
            state.low_demand_since.is_none(),
            "timer restarts after each single-step removal"
        );
    }

    #[test]
    fn scale_down_removes_from_last_cluster_first() {
        let g = group(BTreeMap::from([(Mode::FriSnark, deployment(1, 4))]));
        let queues = fri_queues(0, 0);
        let clusters = [cluster("a", 2, 2, 0), cluster("b", 1, 1, 0)];
        let t0 = Instant::now();
        let mut state = DeploymentState {
            up_streak: 0,
            low_demand_since: Some(t0),
        };

        let decision = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &clusters,
            8,
            &mut state,
            t0 + minutes(30),
        );
        assert!(
            matches!(decision, Decision::Scale { ref cluster, to: 0, .. } if cluster == "b"),
            "{decision:?}"
        );
    }

    #[test]
    fn budget_clamps_desired() {
        let g = group(BTreeMap::from([(Mode::FriSnark, deployment(1, 4))]));
        let queues = fri_queues(8, 0); // backlog alone wants 4
        let clusters = [cluster("a", 2, 2, 0)];
        let mut state = DeploymentState {
            up_streak: 10,
            low_demand_since: None,
        };

        // Budget only allows 2 total → desired == spec → hold at target.
        let decision = plan_once(
            Mode::FriSnark,
            &g,
            Some(&queues),
            &clusters,
            2,
            &mut state,
            Instant::now(),
        );
        assert!(
            matches!(decision, Decision::Hold { ref reason } if reason.contains("at target")),
            "{decision:?}"
        );
    }

    #[test]
    fn stale_queue_holds_and_resets_hysteresis() {
        let g = group(BTreeMap::from([(Mode::FriSnark, deployment(1, 4))]));
        let clusters = [cluster("a", 2, 2, 0)];
        let mut state = DeploymentState {
            up_streak: 3,
            low_demand_since: Some(Instant::now()),
        };

        let decision = plan_once(
            Mode::FriSnark,
            &g,
            None,
            &clusters,
            8,
            &mut state,
            Instant::now(),
        );
        assert!(matches!(decision, Decision::Hold { .. }), "{decision:?}");
        assert_eq!(state.up_streak, 0);
        assert!(state.low_demand_since.is_none());
    }

    #[test]
    fn snark_prewarm_counts_fri_jobs_finishing_during_startup() {
        let mut deployments = BTreeMap::new();
        deployments.insert(Mode::FriOnly, deployment(1, 6)); // FRI p95 = 15m
        let mut snark = deployment(0, 2);
        snark.duration_p95 = minutes(4);
        snark.drain_window = minutes(10);
        snark.scale_up_confirmations = 1;
        deployments.insert(Mode::SnarkOnly, snark);
        let g = group(deployments); // startup_p95 = 8m

        // No SNARK work yet, but 4 FRI jobs in progress: 4 × (8/15) → 2
        // expected completions → backlog ceil(2 × 4m / 10m) = 1 pod pre-warmed.
        let queues = GroupQueues {
            fri: StageQueue {
                ready: 0,
                in_progress: 4,
                reclaimable: 0,
                oldest_ready_age: None,
            },
            snark: StageQueue::default(),
        };
        let clusters = [cluster("a", 0, 0, 0)];
        let mut state = DeploymentState::default();

        let decision = plan_once(
            Mode::SnarkOnly,
            &g,
            Some(&queues),
            &clusters,
            8,
            &mut state,
            Instant::now(),
        );
        assert!(
            matches!(decision, Decision::Scale { to: 1, .. }),
            "{decision:?}"
        );
    }
}
