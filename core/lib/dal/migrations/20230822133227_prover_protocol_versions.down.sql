ALTER TABLE witness_inputs DROP CONSTRAINT IF EXISTS witness_inputs_prover_protocol_version_fkey;
ALTER TABLE witness_inputs ADD CONSTRAINT witness_inputs_protocol_version_fkey
    FOREIGN KEY (protocol_version) REFERENCES protocol_versions (id);

ALTER TABLE leaf_aggregation_witness_jobs DROP CONSTRAINT IF EXISTS leaf_aggregation_witness_jobs_prover_protocol_version_fkey;
ALTER TABLE leaf_aggregation_witness_jobs ADD CONSTRAINT leaf_aggregation_witness_jobs_protocol_version_fkey
    FOREIGN KEY (protocol_version) REFERENCES protocol_versions (id);

ALTER TABLE node_aggregation_witness_jobs DROP CONSTRAINT IF EXISTS node_aggregation_witness_jobs_prover_protocol_version_fkey;
ALTER TABLE node_aggregation_witness_jobs ADD CONSTRAINT node_aggregation_witness_jobs_protocol_version_fkey
    FOREIGN KEY (protocol_version) REFERENCES protocol_versions (id);

ALTER TABLE scheduler_witness_jobs DROP CONSTRAINT IF EXISTS scheduler_witness_jobs_prover_protocol_version_fkey;
ALTER TABLE scheduler_witness_jobs ADD CONSTRAINT scheduler_witness_jobs_protocol_version_fkey
    FOREIGN KEY (protocol_version) REFERENCES protocol_versions (id);

ALTER TABLE prover_jobs DROP CONSTRAINT IF EXISTS prover_jobs_prover_protocol_version_fkey;
ALTER TABLE prover_jobs ADD CONSTRAINT prover_jobs_protocol_version_fkey
    FOREIGN KEY (protocol_version) REFERENCES protocol_versions (id);

DROP TABLE IF EXISTS prover_protocol_versions;
