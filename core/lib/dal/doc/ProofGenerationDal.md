# ProofGenerationDal
## Table Name
proof_generation_details
## `status` Diagram
```mermaid
---
title: Status Diagram
---
stateDiagram-v2
[*] --> ready_to_be_proven : insert_proof_generation_details
ready_to_be_proven --> picked_by_prover : get_next_block_to_be_proven
picked_by_prover --> generated : save_proof_artifacts_metadata
generated --> [*]
[*] --> skipped : mark_proof_generation_job_as_skipped
skipped --> [*]
```
