---- PROVER FRI PROTOCOL VERSIONS

-- Used for determining which version of the FRI protocol is used for a given batch. Useful for VM upgrades, but currently more of a kludge.
CREATE TABLE IF NOT EXISTS prover_fri_protocol_versions (
    id                                INTEGER NOT NULL PRIMARY KEY,
    recursion_scheduler_level_vk_hash BYTEA NOT NULL,
    recursion_node_level_vk_hash      BYTEA NOT NULL,
    recursion_leaf_level_vk_hash      BYTEA NOT NULL,
    recursion_circuits_set_vks_hash   BYTEA NOT NULL,
    created_at                        TIMESTAMP NOT NULL
);

COMMENT ON TABLE prover_fri_protocol_versions IS 'Used for determining which version of the FRI protocol is used for a given batch. Useful for VM upgrades, but currently more of a kludge.';

---- GPU PROVER QUEUE FRI

-- Serves as "registration" hub for provers. WVGs query this table to know which provers are available and how to reach them.
CREATE TABLE IF NOT EXISTS gpu_prover_queue_fri (
    id                          BIGSERIAL PRIMARY KEY,
    instance_host               INET      NOT NULL,
    instance_port               INT       NOT NULL
        CONSTRAINT valid_port CHECK (instance_port >= 0 AND instance_port <= 65535),
    instance_status             TEXT      NOT NULL,
    specialized_prover_group_id SMALLINT  NOT NULL,
    zone                        TEXT,
    created_at                  TIMESTAMP NOT NULL,
    updated_at                  TIMESTAMP NOT NULL,
    processing_started_at       TIMESTAMP
);

COMMENT ON TABLE gpu_prover_queue_fri IS 'Serves as "registration" hub for provers. WVGs query this table to know which provers are available and how to reach them.';

CREATE UNIQUE INDEX IF NOT EXISTS gpu_prover_queue_fri_host_port_zone_idx
    ON gpu_prover_queue_fri (instance_host, instance_port, zone);
-- Not clear if this index is used or necessary, but as part of the effort to backport everything from mainnet, it is included.
CREATE INDEX IF NOT EXISTS tmp_gpu_prover_queue_fri_t2
    ON gpu_prover_queue_fri (specialized_prover_group_id, zone, updated_at, instance_status, processing_started_at)
    INCLUDE (id);

---- GPU PROVRE QUEUE FRI ARCHIVE

-- Table created during the inscriptions outage. May be further used in similar circumstances or as part of an archival process (to be defined in the future).
CREATE TABLE IF NOT EXISTS gpu_prover_queue_fri_archive (
    id                          BIGSERIAL PRIMARY KEY,
    instance_host               INET      NOT NULL,
    instance_port               INT       NOT NULL
        CONSTRAINT valid_port CHECK (instance_port >= 0 AND instance_port <= 65535),
    instance_status             TEXT      NOT NULL,
    specialized_prover_group_id SMALLINT  NOT NULL,
    zone                        TEXT,
    created_at                  TIMESTAMP NOT NULL,
    updated_at                  TIMESTAMP NOT NULL,
    processing_started_at       TIMESTAMP
);

COMMENT ON TABLE gpu_prover_queue_fri_archive IS 'Table created during the inscriptions outage. May be further used in similar circumstances or as part of an archival process (to be defined in the future).';

---- LEAF AGGREGATION WITNESS JOBS FRI

-- Leaf aggregation jobs. Each is picked by a WG which produces input for provers.
CREATE TABLE IF NOT EXISTS leaf_aggregation_witness_jobs_fri (
    id                          BIGSERIAL PRIMARY KEY,
    l1_batch_number             BIGINT NOT NULL,
    circuit_id                  SMALLINT NOT NULL,
    closed_form_inputs_blob_url TEXT,
    attempts                    SMALLINT NOT NULL DEFAULT 0,
    status                      TEXT NOT NULL,
    error                       TEXT,
    created_at                  TIMESTAMP NOT NULL,
    updated_at                  TIMESTAMP NOT NULL,
    processing_started_at       TIMESTAMP,
    time_taken                  TIME,
    -- Note that is_blob_cleaned column is not used, but as part of just mirroring mainnet prover tables, it'll be kept.
    -- To be deleted with a follow-up migration.
    is_blob_cleaned             BOOLEAN,
    number_of_basic_circuits    INTEGER,
    protocol_version            INT REFERENCES prover_fri_protocol_versions (id),
    picked_by                   TEXT
);

COMMENT ON TABLE leaf_aggregation_witness_jobs_fri IS 'Leaf aggregation jobs. Each is picked by a WG which produces input for provers.';

CREATE UNIQUE INDEX IF NOT EXISTS leaf_aggregation_witness_jobs_fri_composite_index
    ON leaf_aggregation_witness_jobs_fri(l1_batch_number, circuit_id);
CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_witness_jobs_fri_queued_order
    ON leaf_aggregation_witness_jobs_fri (l1_batch_number, id)
    WHERE (status = 'queued'::TEXT);
CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_fri_status_processing_attempts
    ON leaf_aggregation_witness_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY['in_progress'::TEXT, 'failed'::TEXT]));

---- NODE AGGREGATION WITNESS JOBS FRI

CREATE TABLE IF NOT EXISTS node_aggregation_witness_jobs_fri (
    id                       BIGSERIAL PRIMARY KEY,
    l1_batch_number          BIGINT NOT NULL,
    circuit_id               SMALLINT NOT NULL,
    depth                    INT NOT NULL DEFAULT 0,
    status                   TEXT NOT NULL,
    attempts                 SMALLINT NOT NULL DEFAULT 0,
    aggregations_url         TEXT,
    processing_started_at    TIMESTAMP,
    time_taken               TIME,
    error                    TEXT,
    created_at               TIMESTAMP NOT NULL,
    updated_at               TIMESTAMP NOT NULL,
    number_of_dependent_jobs INTEGER,
    protocol_version         INTEGER REFERENCES prover_fri_protocol_versions (id),
    picked_by                TEXT
);

COMMENT ON TABLE node_aggregation_witness_jobs_fri IS 'Node aggregation jobs. Each is picked by a WG which produces input for provers.';

CREATE UNIQUE INDEX IF NOT EXISTS node_aggregation_witness_jobs_fri_composite_index
    ON node_aggregation_witness_jobs_fri (l1_batch_number, circuit_id, depth);
CREATE INDEX IF NOT EXISTS idx_node_aggregation_witness_jobs_fri_queued_order
    ON node_aggregation_witness_jobs_fri (l1_batch_number, depth, id)
    WHERE (status = 'queued'::TEXT);
CREATE INDEX IF NOT EXISTS idx_node_aggregation_fri_status_processing_attempts
    ON node_aggregation_witness_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY['in_progress'::TEXT, 'failed'::TEXT]));

---- PROOF COMPRESSION JOBS FRI

-- Proof compression jobs. Last step, turning the wrapping the STARK proof into a SNARK wrapper. (STARK -> SNARK)
CREATE TABLE IF NOT EXISTS proof_compression_jobs_fri (
    l1_batch_number       BIGINT NOT NULL PRIMARY KEY,
    attempts              SMALLINT DEFAULT 0 NOT NULL,
    status                TEXT NOT NULL,
    fri_proof_blob_url    TEXT,
    l1_proof_blob_url     TEXT,
    error                 TEXT,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken            TIME,
    picked_by             TEXT
);

COMMENT ON TABLE proof_compression_jobs_fri IS 'Proof compression jobs. Last step, turning the wrapping the STARK proof into a SNARK wrapper. (STARK -> SNARK)';

CREATE INDEX IF NOT EXISTS idx_proof_compression_jobs_fri_status_processing_attempts
    ON proof_compression_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY['in_progress'::text, 'failed'::text]));
CREATE INDEX IF NOT EXISTS idx_proof_compressor_jobs_fri_queued_order
    ON proof_compression_jobs_fri (l1_batch_number)
    WHERE (status = 'queued'::text);

---- PROVER JOBS FRI

-- Prover jobs. Each is picked by a WVG and is finished by a prover.
CREATE TABLE IF NOT EXISTS prover_jobs_fri (
    id                    BIGSERIAL PRIMARY KEY,
    l1_batch_number       BIGINT NOT NULL,
    circuit_id            SMALLINT NOT NULL,
    circuit_blob_url      TEXT NOT NULL,
    aggregation_round     SMALLINT NOT NULL,
    sequence_number       INTEGER NOT NULL,
    status                TEXT NOT NULL,
    error                 TEXT,
    attempts              SMALLINT DEFAULT 0 NOT NULL,
    processing_started_at TIMESTAMP,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    time_taken            TIME,
    -- Note that is_blob_cleaned column is not used, but as part of just mirroring mainnet prover tables, it'll be kept.
    -- To be deleted with a follow-up migration.
    is_blob_cleaned       BOOLEAN,
    depth                 INTEGER DEFAULT 0 NOT NULL,
    is_node_final_proof   BOOLEAN DEFAULT FALSE NOT NULL,
    proof_blob_url        TEXT,
    protocol_version      INTEGER REFERENCES prover_fri_protocol_versions (id),
    picked_by             TEXT
);

COMMENT ON TABLE prover_jobs_fri IS 'Prover jobs. Each is picked by a WVG and is finished by a prover.';

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_circuit_id_agg_batch_num
    ON prover_jobs_fri (circuit_id, aggregation_round, l1_batch_number)
    WHERE (status = ANY (ARRAY['queued'::text, 'in_progress'::text, 'in_gpu_proof'::text, 'failed'::text]));
CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order
    ON prover_jobs_fri (aggregation_round DESC, l1_batch_number, id)
    WHERE (status = 'queued'::text);
CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order2
    ON prover_jobs_fri (l1_batch_number, aggregation_round DESC, id)
    WHERE (status = 'queued'::text);
CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_status
    ON prover_jobs_fri (circuit_id, aggregation_round, status)
    WHERE ((status <> 'successful'::text) AND (status <> 'skipped'::text));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_fri_t1
    ON prover_jobs_fri (circuit_id, aggregation_round, l1_batch_number, id)
    WHERE (status = 'queued'::text);
CREATE UNIQUE INDEX IF NOT EXISTS prover_jobs_fri_composite_index
    ON public.prover_jobs_fri (l1_batch_number, aggregation_round, circuit_id, depth, sequence_number);
-- This index looks suspiciously hardcoded, afraid if we update setting it may render it useless. As part of the effort to backport everything from mainnet, it is included, but should be re-thought.
CREATE INDEX IF NOT EXISTS prover_jobs_fri_status_processing_started_at_idx
    ON public.prover_jobs_fri (status, processing_started_at)
    WHERE (attempts < 20);

---- PROVER JOBS FRI ARCHIVE

-- Table created during the inscriptions outage. May be further used in similar circumstances or as part of an archival process (to be defined in the future).
CREATE TABLE IF NOT EXISTS prover_jobs_fri_archive (
    id                    BIGSERIAL PRIMARY KEY,
    l1_batch_number       BIGINT NOT NULL,
    circuit_id            SMALLINT NOT NULL,
    circuit_blob_url      TEXT NOT NULL,
    aggregation_round     SMALLINT NOT NULL,
    sequence_number       INTEGER NOT NULL,
    status                TEXT NOT NULL,
    error                 TEXT,
    attempts              SMALLINT DEFAULT 0 NOT NULL,
    processing_started_at TIMESTAMP,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    time_taken            TIME,
    -- Note that is_blob_cleaned column is not used, but as part of just mirroring mainnet prover tables, it'll be kept.
    -- To be deleted with a follow-up migration.
    is_blob_cleaned       BOOLEAN,
    depth                 INTEGER DEFAULT 0 NOT NULL,
    is_node_final_proof   BOOLEAN DEFAULT FALSE NOT NULL,
    proof_blob_url        TEXT,
    protocol_version      INTEGER REFERENCES prover_fri_protocol_versions (id),
    picked_by             TEXT
);

COMMENT ON TABLE prover_jobs_fri_archive IS 'Table created during the inscriptions outage. May be further used in similar circumstances or as part of an archival process (to be defined in the future).';

---- SCHEDULER DEPENDENCY TRACKER FRI

-- Used to track all node completions, before we can queue the scheduler.
CREATE TABLE IF NOT EXISTS scheduler_dependency_tracker_fri (
    l1_batch_number                BIGINT NOT NULL PRIMARY KEY,
    status                         TEXT NOT NULL,
    circuit_1_final_prover_job_id  BIGINT,
    circuit_2_final_prover_job_id  BIGINT,
    circuit_3_final_prover_job_id  BIGINT,
    circuit_4_final_prover_job_id  BIGINT,
    circuit_5_final_prover_job_id  BIGINT,
    circuit_6_final_prover_job_id  BIGINT,
    circuit_7_final_prover_job_id  BIGINT,
    circuit_8_final_prover_job_id  BIGINT,
    circuit_9_final_prover_job_id  BIGINT,
    circuit_10_final_prover_job_id BIGINT,
    circuit_11_final_prover_job_id BIGINT,
    circuit_12_final_prover_job_id BIGINT,
    circuit_13_final_prover_job_id BIGINT,
    created_at                     TIMESTAMP NOT NULL,
    updated_at                     TIMESTAMP NOT NULL
);

COMMENT ON TABLE scheduler_dependency_tracker_fri IS 'Used to track all node completions, before we can queue the scheduler.';

CREATE INDEX IF NOT EXISTS idx_scheduler_dependency_tracker_fri_circuit_ids_filtered
    ON public.scheduler_dependency_tracker_fri (circuit_1_final_prover_job_id, circuit_2_final_prover_job_id, circuit_3_final_prover_job_id, circuit_4_final_prover_job_id, circuit_5_final_prover_job_id, circuit_6_final_prover_job_id, circuit_7_final_prover_job_id, circuit_8_final_prover_job_id, circuit_9_final_prover_job_id, circuit_10_final_prover_job_id, circuit_11_final_prover_job_id, circuit_12_final_prover_job_id, circuit_13_final_prover_job_id)
    WHERE (status <> 'queued'::text);

---- SCHEDULER WITNESS JOBS FRI

---- Scheduler jobs. Each is picked by a WG which produces input for provers.
CREATE TABLE IF NOT EXISTS scheduler_witness_jobs_fri (
    l1_batch_number                  BIGINT NOT NULL PRIMARY KEY,
    scheduler_partial_input_blob_url TEXT NOT NULL,
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

COMMENT ON TABLE scheduler_witness_jobs_fri IS 'Scheduler jobs. Each is picked by a WG which produces input for provers.';

CREATE INDEX IF NOT EXISTS idx_scheduler_fri_status_processing_attempts
    ON public.scheduler_witness_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY['in_progress'::text, 'failed'::text]));

---- WITNESS INPUTS FRI

-- Witness input jobs. Each is picked by a WG which produces input for provers.
CREATE TABLE IF NOT EXISTS witness_inputs_fri (
    l1_batch_number            BIGINT NOT NULL PRIMARY KEY,
    merkle_tree_paths_blob_url TEXT,
    attempts                   SMALLINT DEFAULT 0 NOT NULL,
    status                     TEXT NOT NULL,
    error                      TEXT,
    created_at                 TIMESTAMP NOT NULL,
    updated_at                 TIMESTAMP NOT NULL,
    processing_started_at      TIMESTAMP,
    time_taken                 TIME,
    -- Note that is_blob_cleaned column is not used, but as part of just mirroring mainnet prover tables, it'll be kept.
    -- To be deleted with a follow-up migration.
    is_blob_cleaned            BOOLEAN,
    protocol_version           INTEGER REFERENCES prover_fri_protocol_versions (id),
    picked_by                  TEXT
);

COMMENT ON TABLE witness_inputs_fri IS 'Witness input jobs. Each is picked by a WG which produces input for provers.';

CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_status_processing_attempts
    ON witness_inputs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY['in_progress'::text, 'failed'::text]));
CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_queued_order
    ON witness_inputs_fri (l1_batch_number)
    WHERE (status = 'queued'::text);
