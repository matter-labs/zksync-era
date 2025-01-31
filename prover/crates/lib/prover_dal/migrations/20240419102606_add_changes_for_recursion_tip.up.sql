DROP TABLE IF EXISTS scheduler_dependency_tracker_fri;

CREATE TABLE IF NOT EXISTS recursion_tip_witness_jobs_fri (
    l1_batch_number           BIGINT PRIMARY KEY,
    status                    TEXT NOT NULL,
    attempts                  SMALLINT NOT NULL DEFAULT 0,
    processing_started_at     TIMESTAMP,
    time_taken                TIME,
    error                     TEXT,
    created_at                TIMESTAMP NOT NULL,
    updated_at                TIMESTAMP NOT NULL,
    number_of_final_node_jobs INTEGER,
    protocol_version          INTEGER REFERENCES prover_fri_protocol_versions (id),
    picked_by                 TEXT
);

COMMENT ON TABLE recursion_tip_witness_jobs_fri IS 'Recursion tip jobs. Each is picked by a WG which produces input for provers. Only 1 per batch, last step before scheduler.';

-- This index helps with rescheduling recursion tip jobs
CREATE INDEX IF NOT EXISTS recursion_tip_witness_jobs_fri_status_processing_attempts_idx
    ON recursion_tip_witness_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY['in_progress'::TEXT, 'failed'::TEXT]));

-- This index helps with getting jobs necessary for recursion_tip_witness_jobs_fri
CREATE INDEX IF NOT EXISTS prover_jobs_fri_l1_batch_number_is_node_final_proof_true_status_successful_idx ON prover_jobs_fri (l1_batch_number, is_node_final_proof, status)
WHERE is_node_final_proof = true AND status = 'successful'::TEXT;
