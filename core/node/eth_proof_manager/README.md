# ETH Proof Manager

ETH Proof Manager is a component dedicated for management of batches/proofs for proving networks.

## Database

Component has its separate table for management. Table has the following properties:

- `l1_batch_number` - actual batch number that needs to be proven
- `status` - current status of batch. Statuses can be the following:
  - `ready_to_be_proven` - that means that the proof generation data for a batch is ready and we are fine to send it to
    L1
  - `request_sent_to_l1` - we have successfully sent proof request to L1 contract
  - `acknowledged` - proving network acknowledged the proof request
  - `received_proof` - received proof for a batch
  - `validation_result_sent_to_l1` - we have successfully sent validation result to L1 contract
- `proof_gen_data_blob_url` - URL of proof generation data blob for the batch
- `proof_blob_url` - URL of the received proof(when saved to our GCS)
- `assigned_to` - when the proof is aknowledged this field is filled with a proving network that aknowledged the proof
  request
- `sent_proof_request_tx_hash` - transaction hash of sending proof request tx that is being sent to L1 contract(useful
  for debugging purposes)
- `sent_proof_validation_result_tx_hash` - transaction hash of proof validation result
- `validation_status` - Current status of proof validation. Statuses can be the following:
  - `successful`
  - `failed`

## Structure

The component consists of 3 subcomponents:

- ReadinessChecker
- Watcher
- Sender

## ReadinessChecker

Orchestrator consists of multiple tasks:

- ReadinessChecker - checks if the batch is ready to be proven against `proof_generation_details` table. If so, it first
  saves the blob into public blob storage and updates the `eth_proof_manager` table, signaling about batch readiness.

## Sender

Sender periodically checks for batches that needs to be sent to L1. Sender calls the next methods on L1 contract:

- `submitProofRequest`
- `submitProofValidationResult`

## Watcher

Watcher subcomponent is responsible for listening to events emitted by L1 contract. These events include:

- `ProofRequestAcknowledged` - this event means that Proof Request was acknowledged by a proving network. Receiving such
  event leads to the next sequence of operations:
  - Changing status of the batch to `acknowledged`.
  - `assigned_to` column is updated with corresponding chain that acknowledged batch.
- ## `ProofRequestProven` - this event means that proving network has successfully proven a batch
- `RewardClaimed` - this event means that proving network claimed their reward. Used for statistical purposes.
