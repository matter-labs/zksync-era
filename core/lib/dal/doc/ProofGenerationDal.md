# ProofGenerationDal

## Table Name

proof_generation_details

## `status` Diagram

```mermaid
---
title: Status Diagram
---
stateDiagram-v2
[*] --> unpicked : insert_proof_generation_details
unpicked --> picked_by_prover : lock_batch_for_proving
picked_by_prover --> generated : save_proof_artifacts_metadata
picked_by_prover --> unpicked : unlock_batch
generated --> [*]

[*] --> skipped : mark_proof_generation_job_as_skipped
skipped --> [*]

```
