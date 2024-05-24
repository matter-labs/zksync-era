ALTER TABLE witness_inputs_fri
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE leaf_aggregation_witness_jobs_fri
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE node_aggregation_witness_jobs_fri
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE recursion_tip_witness_jobs_fri
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE scheduler_witness_jobs_fri
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE proof_compression_jobs_fri
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE prover_jobs_fri
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE prover_jobs_fri_archive
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE gpu_prover_queue_fri
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE gpu_prover_queue_fri_archive
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;
