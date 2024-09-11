ALTER TABLE proof_compression_jobs_fri
    ADD COLUMN protocol_version INTEGER REFERENCES prover_fri_protocol_versions (id) DEFAULT NULL;

ALTER TABLE gpu_prover_queue_fri
    ADD COLUMN protocol_version INTEGER REFERENCES prover_fri_protocol_versions (id) DEFAULT NULL;

ALTER TABLE gpu_prover_queue_fri_archive
    ADD COLUMN protocol_version INTEGER REFERENCES prover_fri_protocol_versions (id) DEFAULT NULL;
