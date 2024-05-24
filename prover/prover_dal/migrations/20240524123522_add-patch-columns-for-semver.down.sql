ALTER TABLE witness_inputs_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE leaf_aggregation_witness_jobs_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE node_aggregation_witness_jobs_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE recursion_tip_witness_jobs_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE scheduler_witness_jobs_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE proof_compression_jobs_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE prover_jobs_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE prover_jobs_fri_archive
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE gpu_prover_queue_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE gpu_prover_queue_fri_archive
    DROP IF EXISTS protocol_version_patch;
