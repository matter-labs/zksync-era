ALTER TABLE prover_fri_protocol_versions
    DROP CONSTRAINT prover_fri_protocol_versions_pkey CASCADE;

ALTER TABLE witness_inputs_fri
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE leaf_aggregation_witness_jobs_fri
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE node_aggregation_witness_jobs_fri
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE recursion_tip_witness_jobs_fri
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE scheduler_witness_jobs_fri
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE proof_compression_jobs_fri
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE prover_jobs_fri
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE prover_jobs_fri_archive
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE gpu_prover_queue_fri
    DROP IF EXISTS protocol_version_patch;

ALTER TABLE gpu_prover_queue_fri_archive
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE prover_fri_protocol_versions
    DROP COLUMN IF EXISTS protocol_version_patch;

ALTER TABLE prover_fri_protocol_versions
    ADD CONSTRAINT prover_fri_protocol_versions_pkey PRIMARY KEY (id);

ALTER TABLE witness_inputs_fri
    ADD CONSTRAINT witness_inputs_fri_protocol_version_fkey
        FOREIGN KEY (protocol_version)
            REFERENCES prover_fri_protocol_versions (id);

ALTER TABLE leaf_aggregation_witness_jobs_fri
    ADD CONSTRAINT leaf_aggregation_witness_jobs_fri_protocol_version_fkey
        FOREIGN KEY (protocol_version)
            REFERENCES prover_fri_protocol_versions (id);

ALTER TABLE node_aggregation_witness_jobs_fri
    ADD CONSTRAINT node_aggregation_witness_jobs_fri_protocol_version_fkey
        FOREIGN KEY (protocol_version) REFERENCES prover_fri_protocol_versions (id);

ALTER TABLE recursion_tip_witness_jobs_fri
    ADD CONSTRAINT recursion_tip_witness_jobs_fri_protocol_version_fkey
        FOREIGN KEY (protocol_version) REFERENCES prover_fri_protocol_versions (id);

ALTER TABLE scheduler_witness_jobs_fri
    ADD CONSTRAINT scheduler_witness_jobs_fri_protocol_version_fkey
        FOREIGN KEY (protocol_version)
            REFERENCES prover_fri_protocol_versions (id);

ALTER TABLE proof_compression_jobs_fri
    ADD CONSTRAINT proof_compression_jobs_fri_protocol_version_fkey
        FOREIGN KEY (protocol_version)
            REFERENCES prover_fri_protocol_versions (id);

ALTER TABLE prover_jobs_fri
    ADD CONSTRAINT prover_jobs_fri_protocol_version_fkey
        FOREIGN KEY (protocol_version)
            REFERENCES prover_fri_protocol_versions (id);
