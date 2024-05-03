CREATE TABLE IF NOT EXISTS prover_fri_protocol_versions
(
    id                                INT PRIMARY KEY,
    recursion_scheduler_level_vk_hash BYTEA     NOT NULL,
    recursion_node_level_vk_hash      BYTEA     NOT NULL,
    recursion_leaf_level_vk_hash      BYTEA     NOT NULL,
    recursion_circuits_set_vks_hash   BYTEA     NOT NULL,
    created_at                        TIMESTAMP NOT NULL
);

ALTER TABLE witness_inputs_fri ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES prover_fri_protocol_versions (id);
ALTER TABLE leaf_aggregation_witness_jobs_fri ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES prover_fri_protocol_versions (id);
ALTER TABLE node_aggregation_witness_jobs_fri ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES prover_fri_protocol_versions (id);
ALTER TABLE scheduler_witness_jobs_fri ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES prover_fri_protocol_versions (id);
ALTER TABLE prover_jobs_fri ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES prover_fri_protocol_versions (id);
