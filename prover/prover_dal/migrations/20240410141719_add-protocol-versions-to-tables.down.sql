ALTER TABLE proof_compression_jobs_fri
    DROP IF EXISTS protocol_version;

ALTER TABLE gpu_prover_queue_fri
    DROP IF EXISTS protocol_version;

ALTER TABLE gpu_prover_queue_fri_archive
    DROP IF EXISTS protocol_version;
