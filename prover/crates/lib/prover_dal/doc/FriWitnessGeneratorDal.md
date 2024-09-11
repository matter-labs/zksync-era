# FriWitnessGeneratorDal

## Table Name

witness_inputs_fri leaf_aggregation_witness_jobs_fri node_aggregation_witness_jobs_fri scheduler_witness_jobs_fri
scheduler_dependency_tracker_fri

### witness_inputs_fri

#### `status` Diagram

```mermaid
---
title: Status Diagram
---
stateDiagram-v2
[*] --> queued : save_witness_inputs
queued --> in_progress : get_next_basic_circuit_witness_job
in_progress --> successful : mark_witness_job_as_successful
successful --> [*]
in_progress --> failed : mark_witness_job_failed
failed --> queued : requeue_stuck_jobs
in_progress --> queued : requeue_stuck_jobs
```

### leaf_aggregation_witness_jobs_fri

#### `status` Diagram

```mermaid
---
title: Status Diagram
---
stateDiagram-v2
[*] --> waiting_for_proofs : create_aggregation_jobs
waiting_for_proofs --> queued : move_leaf_aggregation_jobs_from_waiting_to_queued
queued --> in_progress : get_next_leaf_aggregation_job
in_progress --> successful : mark_leaf_aggregation_as_successful
successful --> [*]
in_progress --> failed : mark_leaf_aggregation_job_failed
failed --> queued : requeue_stuck_leaf_aggregations_jobs
in_progress --> queued : requeue_stuck_leaf_aggregations_jobs

successful --> [*]

```

### node_aggregation_witness_jobs_fri

#### `status` Diagram

```mermaid
---
title: Status Diagram
---
stateDiagram-v2
[*] --> waiting_for_proofs : insert_node_aggregation_jobs
waiting_for_proofs --> queued : move_depth_zero_node_aggregation_jobs
    queued --> in_progress : get_next_node_aggregation_job
in_progress --> successful : mark_node_aggregation_as_successful
successful --> [*]
in_progress --> failed : mark_node_aggregation_job_failed
failed --> queued : requeue_stuck_node_aggregations_jobs
in_progress --> queued : requeue_stuck_node_aggregations_jobs

```

### scheduler_witness_jobs_fri

#### `status` Diagram

```mermaid
---
title: Status Diagram
---
stateDiagram-v2
[*] --> queued: create_aggregation_jobs
queued --> in_progress: get_next_scheduler_witness_job
in_progress --> successful: mark_scheduler_job_as_successful
successful --> [*]
in_progress --> failed: mark_scheduler_job_failed
failed --> queued: requeue_stuck_scheduler_jobs
in_progress --> queued: requeue_stuck_scheduler_jobs

```
