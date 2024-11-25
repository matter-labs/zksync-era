# TeeProofGenerationDal

## Table Name

`tee_proofs`

## `status` Diagram

```mermaid
---
title: Status Diagram
---
stateDiagram-v2
[*] --> picked_by_prover : lock
picked_by_prover --> generated : save_proof_artifacts_metadata
picked_by_prover --> permanently_ignored : unlock_batch
picked_by_prover --> failed : unlock_batch
failed --> picked_by_prover : lock
permanently_ignored --> [*]
generated --> [*]
```
