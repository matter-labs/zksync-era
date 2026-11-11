# Airbender Autoscaler

From-scratch autoscaler for Airbender prover fleets. A single binary, standalone workspace, **no zksync crate
dependencies** — it talks to chain proof data handlers over HTTP and to Kubernetes API servers via kube-rs. It replaces
nothing in-place: the legacy `prover_autoscaler` (agents, GPU-type ranking, aggressive mode) keeps serving the old proof
system until retired.

## How it works

A Kubernetes-style reconcile loop, every `reconcile_interval`:

```text
chain proof handlers ──/airbender/queue_report──►┐
K8s API (per cluster) ──deployments + pods──────►│ Snapshot ─► Planner (pure fn) ─► Actuator (patch scale)
```

- **Observe** — poll every chain of a compatibility group for its queue report and aggregate; read each deployment's
  `spec.replicas` and pod phases (Running / Pending with age / Terminating) per cluster.
- **Plan** — `planner::plan` is a pure function; all policy lives there and is unit-tested (`cargo test`). Dry-run mode
  runs the identical code and skips only the actuator.
- **Act** — patch the Deployment scale subresource, one replica at a time.

### Scaling policy (drain horizon)

```text
work      = ready + in_progress + reclaimable    (+ pre-warm for snark-only)
n_backlog = ceil(work × duration_p95 / drain_window)
desired   = clamp(n_backlog, min, max)           then the group max_gpus budget
```

- Scale-up: +1 replica after `scale_up_confirmations` consecutive over-threshold ticks, bypassed when the oldest ready
  job breaches `latency_slo`.
- Scale-down: −1 replica after `scale_down_stabilization` of sustained low demand, only when the fleet is settled (no
  Pending/Terminating pods); the timer restarts after every removal.
- `snark-only` pre-warms: FRI jobs expected to finish within `startup_p95` count as SNARK work, so wraps don't eat a
  cold start.

### Scarce machines, single card class

`spec.replicas` is treated as capacity already requested: a Pending pod is never re-requested, and each missing replica
has exactly one outstanding attempt. A cluster with a pod Pending longer than `startup_p95` is starved; scale-up moves
to the next configured cluster, and when all are starved the planner holds and says so (`capacity starved`) — with one
GPU card class there is no fallback pool to race, by design. `max_gpus` is a hard budget per group; under budget
pressure, deployments are funded in the order `snark-only`, `fri-snark`, `fri-only` so FRI proofs never outrun wrap
capacity.

### Topology is manual

Which deployments exist is the operator's choice in the config, not the scaler's: combined (`fri-snark` only) or split
(`fri-only` + `snark-only`). Config validation rejects `fri-only` without SNARK-capable capacity.

## Running

```bash
cargo run -- --config config.example.yaml --dry-run --once   # one explained tick
cargo run -- --config config.yaml                            # controller loop
cargo test                                                   # planner tests
```

Deployments must already exist (helm/manifests, 0 replicas is fine) with the right prover `--mode`, image, and
nodeSelector; the autoscaler only scales them. Give FRI-capable pods `terminationGracePeriodSeconds` ≈ 30m so a
SIGTERM'd worker finishes and submits its in-flight proof.

## Prerequisites elsewhere (not in this crate)

1. **Queue report endpoint** on every chain's proof data handler: `GET /airbender/queue_report` returning per-stage
   `{ready, in_progress, reclaimable, oldest_ready_at}` whose predicates mirror the DAL locking queries exactly.
2. **Prover server**: SNARK lease on local wrap (`submit_fri` with `will_wrap_locally` → `picked_for_snark`), idle SNARK
   fallback in `fri-snark` mode, and a SIGTERM drain that also submits the local SNARK follow-up (prefetch disabled in
   autoscaled workers).

## Draft status / not yet implemented

- Prometheus metrics (decisions are structured log lines for now).
- Steady-state term (`arrival_rate × duration / utilization`) — the planner is backlog-only until arrival-rate
  estimation is added.
- Persisted hysteresis state (restart re-confirms demand; conservative).
- Pod-deletion-cost hints for scale-down victim selection (graceful drain makes any victim safe).
