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

ALTER TABLE prover_fri_protocol_versions
    ADD COLUMN protocol_version_patch INT NOT NULL DEFAULT 0;

ALTER TABLE prover_fri_protocol_versions
    DROP CONSTRAINT prover_fri_protocol_versions_pkey CASCADE;

ALTER TABLE prover_fri_protocol_versions
    ADD CONSTRAINT protocol_version_pk PRIMARY KEY (id, protocol_version_patch);

ALTER TABLE witness_inputs_fri
    ADD CONSTRAINT protocol_semantic_version_fk
        FOREIGN KEY (protocol_version, protocol_version_patch)
            REFERENCES prover_fri_protocol_versions (id, protocol_version_patch);

ALTER TABLE leaf_aggregation_witness_jobs_fri
    ADD CONSTRAINT protocol_semantic_version_fk
        FOREIGN KEY (protocol_version, protocol_version_patch)
            REFERENCES prover_fri_protocol_versions (id, protocol_version_patch);

ALTER TABLE node_aggregation_witness_jobs_fri
    ADD CONSTRAINT protocol_semantic_version_fk
        FOREIGN KEY (protocol_version, protocol_version_patch)
            REFERENCES prover_fri_protocol_versions (id, protocol_version_patch);

ALTER TABLE recursion_tip_witness_jobs_fri
    ADD CONSTRAINT protocol_semantic_version_fk
        FOREIGN KEY (protocol_version, protocol_version_patch)
            REFERENCES prover_fri_protocol_versions (id, protocol_version_patch);

ALTER TABLE scheduler_witness_jobs_fri
    ADD CONSTRAINT protocol_semantic_version_fk
        FOREIGN KEY (protocol_version, protocol_version_patch)
            REFERENCES prover_fri_protocol_versions (id, protocol_version_patch);

ALTER TABLE proof_compression_jobs_fri
    ADD CONSTRAINT protocol_semantic_version_fk
        FOREIGN KEY (protocol_version, protocol_version_patch)
            REFERENCES prover_fri_protocol_versions (id, protocol_version_patch);

ALTER TABLE prover_jobs_fri
    ADD CONSTRAINT protocol_semantic_version_fk
        FOREIGN KEY (protocol_version, protocol_version_patch)
            REFERENCES prover_fri_protocol_versions (id, protocol_version_patch);
