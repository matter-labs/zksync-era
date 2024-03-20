-- create prover_fri_protocol_versions table
CREATE TABLE IF NOT EXISTS prover_fri_protocol_versions
(
    id                                integer   NOT NULL
        primary key,
    recursion_scheduler_level_vk_hash bytea     NOT NULL,
    recursion_node_level_vk_hash      bytea     NOT NULL,
    recursion_leaf_level_vk_hash      bytea     NOT NULL,
    recursion_circuits_set_vks_hash   bytea     NOT NULL,
    created_at                        timestamp NOT NULL
);

-- create gpu_prover_queue_fri table
CREATE TABLE IF NOT EXISTS gpu_prover_queue_fri
(
    id                          bigserial
        primary key,
    instance_host               inet      NOT NULL,
    instance_port               integer   NOT NULL
        CONSTRAINT valid_port
            CHECK ((instance_port >= 0) AND (instance_port <= 65535)),
    instance_status             text      NOT NULL,
    specialized_prover_group_id smallint  NOT NULL,
    zone                        text,
    created_at                  timestamp NOT NULL,
    updated_at                  timestamp NOT NULL,
    processing_started_at       timestamp
);

CREATE UNIQUE INDEX gpu_prover_queue_fri_host_port_zone_idx
    ON gpu_prover_queue_fri (instance_host, instance_port, zone);

-- create leaf_aggregation_witness_jobs_fri table
CREATE TABLE IF NOT EXISTS leaf_aggregation_witness_jobs_fri
(
    id                          bigserial
        primary key,
    l1_batch_number             bigint             NOT NULL,
    circuit_id                  smallint           NOT NULL,
    closed_form_inputs_blob_url text,
    attempts                    smallint default 0 NOT NULL,
    status                      text               NOT NULL,
    error                       text,
    created_at                  timestamp          NOT NULL,
    updated_at                  timestamp          NOT NULL,
    processing_started_at       timestamp,
    time_taken                  time,
    is_blob_cleaned             boolean,
    number_of_basic_circuits    integer,
    protocol_version            integer
        references prover_fri_protocol_versions,
    picked_by                   text
);

CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_witness_jobs_fri_queued_order
    ON leaf_aggregation_witness_jobs_fri (l1_batch_number, id)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_fri_status_processing_attempts
    ON leaf_aggregation_witness_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY ['in_progress'::text, 'failed'::text]));

CREATE UNIQUE INDEX IF NOT EXISTS leaf_aggregation_witness_jobs_fri_composite_index_1
    ON leaf_aggregation_witness_jobs_fri (l1_batch_number, circuit_id) include (protocol_version);

-- create node_aggregation_witness_jobs_fri table
CREATE TABLE IF NOT EXISTS node_aggregation_witness_jobs_fri
(
    id                       bigserial
        primary key,
    l1_batch_number          bigint             NOT NULL,
    circuit_id               smallint           NOT NULL,
    depth                    integer  default 0 NOT NULL,
    status                   text               NOT NULL,
    attempts                 smallint default 0 NOT NULL,
    aggregations_url         text,
    processing_started_at    timestamp,
    time_taken               time,
    error                    text,
    created_at               timestamp          NOT NULL,
    updated_at               timestamp          NOT NULL,
    number_of_dependent_jobs integer,
    protocol_version         integer
        references prover_fri_protocol_versions,
    picked_by                text
);

CREATE INDEX IF NOT EXISTS idx_node_aggregation_witness_jobs_fri_queued_order
    ON node_aggregation_witness_jobs_fri (l1_batch_number, depth, id)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_node_aggregation_fri_status_processing_attempts
    ON node_aggregation_witness_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY ['in_progress'::text, 'failed'::text]));

CREATE UNIQUE INDEX IF NOT EXISTS node_aggregation_witness_jobs_fri_composite_index_1
    ON node_aggregation_witness_jobs_fri (l1_batch_number, circuit_id, depth) include (protocol_version);

-- create proof_compression_jobs_fri table
CREATE TABLE IF NOT EXISTS proof_compression_jobs_fri
(
    l1_batch_number       bigint             NOT NULL
        primary key,
    attempts              smallint default 0 NOT NULL,
    status                text               NOT NULL,
    fri_proof_blob_url    text,
    l1_proof_blob_url     text,
    error                 text,
    created_at            timestamp          NOT NULL,
    updated_at            timestamp          NOT NULL,
    processing_started_at timestamp,
    time_taken            time,
    picked_by             text
);

CREATE INDEX IF NOT EXISTS idx_proof_compression_jobs_fri_status_processing_attempts
    ON proof_compression_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY ['in_progress'::text, 'failed'::text]));

CREATE INDEX IF NOT EXISTS idx_proof_compressor_jobs_fri_queued_order
    ON proof_compression_jobs_fri (l1_batch_number)
    WHERE (status = 'queued'::text);

-- create prover_jobs_fri table
CREATE TABLE IF NOT EXISTS prover_jobs_fri
(
    id                    bigserial
        primary key,
    l1_batch_number       bigint                 NOT NULL,
    circuit_id            smallint               NOT NULL,
    circuit_blob_url      text                   NOT NULL,
    aggregation_round     smallint               NOT NULL,
    sequence_number       integer                NOT NULL,
    status                text                   NOT NULL,
    error                 text,
    attempts              smallint default 0     NOT NULL,
    processing_started_at timestamp,
    created_at            timestamp              NOT NULL,
    updated_at            timestamp              NOT NULL,
    time_taken            time,
    is_blob_cleaned       boolean,
    depth                 integer  default 0     NOT NULL,
    is_node_final_proof   boolean  default false NOT NULL,
    proof_blob_url        text,
    protocol_version      integer
        references prover_fri_protocol_versions,
    picked_by             text
);

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order
    ON prover_jobs_fri (aggregation_round desc, l1_batch_number asc, id asc)
    WHERE (status = 'queued'::text);

CREATE UNIQUE INDEX IF NOT EXISTS prover_jobs_fri_composite_index_1
    ON prover_jobs_fri (l1_batch_number, aggregation_round, circuit_id, depth,
                        sequence_number) include (protocol_version);

CREATE INDEX IF NOT EXISTS prover_jobs_fri_status_processing_started_at_idx_2
    ON prover_jobs_fri (status, processing_started_at)
    WHERE (attempts < 20);

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_status
    ON prover_jobs_fri (circuit_id, aggregation_round)
    WHERE ((status <> 'successful'::text) AND (status <> 'skipped'::text));

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_circuit_id_agg_batch_num
    ON prover_jobs_fri (circuit_id, aggregation_round, l1_batch_number)
    WHERE (status = ANY (ARRAY ['queued'::text, 'in_progress'::text, 'in_gpu_proof'::text, 'failed'::text]));

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order2
    ON prover_jobs_fri (l1_batch_number asc, aggregation_round desc, id asc)
    WHERE (status = 'queued'::text);

-- create scheduler_dependency_tracker_fri table
CREATE TABLE IF NOT EXISTS scheduler_dependency_tracker_fri
(
    l1_batch_number                bigint    NOT NULL
        primary key,
    status                         text      NOT NULL,
    circuit_1_final_prover_job_id  bigint,
    circuit_2_final_prover_job_id  bigint,
    circuit_3_final_prover_job_id  bigint,
    circuit_4_final_prover_job_id  bigint,
    circuit_5_final_prover_job_id  bigint,
    circuit_6_final_prover_job_id  bigint,
    circuit_7_final_prover_job_id  bigint,
    circuit_8_final_prover_job_id  bigint,
    circuit_9_final_prover_job_id  bigint,
    circuit_10_final_prover_job_id bigint,
    circuit_11_final_prover_job_id bigint,
    circuit_12_final_prover_job_id bigint,
    circuit_13_final_prover_job_id bigint,
    created_at                     timestamp NOT NULL,
    updated_at                     timestamp NOT NULL,
    eip_4844_final_prover_job_id_0 bigint,
    eip_4844_final_prover_job_id_1 bigint
);

CREATE INDEX IF NOT EXISTS idx_scheduler_dependency_tracker_fri_circuit_ids_filtered
    ON scheduler_dependency_tracker_fri (circuit_1_final_prover_job_id, circuit_2_final_prover_job_id,
                                         circuit_3_final_prover_job_id, circuit_4_final_prover_job_id,
                                         circuit_5_final_prover_job_id, circuit_6_final_prover_job_id,
                                         circuit_7_final_prover_job_id, circuit_8_final_prover_job_id,
                                         circuit_9_final_prover_job_id, circuit_10_final_prover_job_id,
                                         circuit_11_final_prover_job_id, circuit_12_final_prover_job_id,
                                         circuit_13_final_prover_job_id)
    WHERE (status <> 'queued'::text);

-- create scheduler_witness_jobs_fri table
CREATE TABLE IF NOT EXISTS scheduler_witness_jobs_fri
(
    l1_batch_number                  bigint             NOT NULL
        primary key,
    scheduler_partial_input_blob_url text               NOT NULL,
    status                           text               NOT NULL,
    processing_started_at            timestamp,
    time_taken                       time,
    error                            text,
    created_at                       timestamp          NOT NULL,
    updated_at                       timestamp          NOT NULL,
    attempts                         smallint default 0 NOT NULL,
    protocol_version                 integer
        references prover_fri_protocol_versions,
    picked_by                        text
);

CREATE INDEX IF NOT EXISTS idx_scheduler_fri_status_processing_attempts
    ON scheduler_witness_jobs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY ['in_progress'::text, 'failed'::text]));

CREATE UNIQUE INDEX IF NOT EXISTS scheduler_witness_jobs_fri_composite_index_1
    ON scheduler_witness_jobs_fri (l1_batch_number) include (protocol_version);

-- create witness_inputs_fri table
CREATE TABLE IF NOT EXISTS witness_inputs_fri
(
    l1_batch_number            bigint             NOT NULL
        primary key,
    merkle_tree_paths_blob_url text,
    attempts                   smallint default 0 NOT NULL,
    status                     text               NOT NULL,
    error                      text,
    created_at                 timestamp          NOT NULL,
    updated_at                 timestamp          NOT NULL,
    processing_started_at      timestamp,
    time_taken                 time,
    is_blob_cleaned            boolean,
    protocol_version           integer
        REFERENCES prover_fri_protocol_versions,
    picked_by                  text,
    eip_4844_blobs             bytea
);

CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_status_processing_attempts
    ON witness_inputs_fri (processing_started_at, attempts)
    WHERE (status = ANY (ARRAY ['in_progress'::text, 'failed'::text]));

CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_queued_order
    ON witness_inputs_fri (l1_batch_number)
    WHERE (status = 'queued'::text);

CREATE TABLE aggregated_proof
(
    id                serial
        primary key,
    from_block_number bigint
        references l1_batches
            on delete cascade,
    to_block_number   bigint
        references l1_batches
            on delete cascade,
    proof             bytea,
    eth_prove_tx_id   integer
                                references eth_txs
                                    on delete set null,
    created_at        timestamp not null
);

CREATE INDEX aggregated_proof_from_l1_batch_index
    ON aggregated_proof (from_block_number);

CREATE INDEX aggregated_proof_to_l1_batch_index
    ON aggregated_proof (to_block_number);

CREATE TABLE proof
(
    l1_batch_number bigint    not null
        primary key
        references l1_batches
            on delete cascade,
    proof           bytea,
    created_at      timestamp not null
);
