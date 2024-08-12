# TeeProofGenerationDal

## Table Name

`tee_proofs`

## `status` Diagram

```mermaid
---
title: Status Diagram
---
stateDiagram-v2
[*] --> ready_to_be_proven : insert_tee_proof_generation_job
ready_to_be_proven --> picked_by_prover : get_next_batch_to_be_proven
picked_by_prover --> generated : save_proof_artifacts_metadata
generated --> [*]

```
