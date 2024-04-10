---- Scheduler jobs. Each is picked by a WG which produces input for provers.
CREATE TABLE IF NOT EXISTS recursion_tip_witness_jobs_fri (
    l1_batch_number                  BIGINT NOT NULL PRIMARY KEY,
    recursion_tip_input_blob_url     TEXT NOT NULL,
    status                           TEXT NOT NULL,
    processing_started_at            TIMESTAMP,
    time_taken                       TIME,
    error                            TEXT,
    created_at                       TIMESTAMP NOT NULL,
    updated_at                       TIMESTAMP NOT NULL,
    attempts                         SMALLINT DEFAULT 0 NOT NULL,
    protocol_version                 INTEGER REFERENCES prover_fri_protocol_versions (id),
    picked_by                        TEXT
);

CREATE INDEX IF NOT EXISTS idx_recursion_tip_fri_status_processing_attempts
    ON public.recursion_tip_witness_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY['in_progress'::text, 'failed'::text]));

ALTER TABLE scheduler_dependency_tracker_fri ADD COLUMN IF NOT EXISTS circuit_14_final_prover_job_id BIGINT;
ALTER TABLE scheduler_dependency_tracker_fri ADD COLUMN IF NOT EXISTS circuit_15_final_prover_job_id BIGINT;
ALTER TABLE scheduler_dependency_tracker_fri ADD COLUMN IF NOT EXISTS circuit_16_final_prover_job_id BIGINT;