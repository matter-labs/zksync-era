CREATE TABLE IF NOT EXISTS protocol_versions (
    id INTEGER NOT NULL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    recursion_scheduler_level_vk_hash BYTEA NOT NULL,
    recursion_node_level_vk_hash BYTEA NOT NULL,
    recursion_leaf_level_vk_hash BYTEA NOT NULL,
    recursion_circuits_set_vks_hash BYTEA NOT NULL,
    bootloader_code_hash BYTEA NOT NULL,
    default_account_code_hash BYTEA NOT NULL,
    verifier_address BYTEA NOT NULL,
    upgrade_tx_hash BYTEA,
    created_at TIMESTAMP NOT NULL
);

CREATE TABLE IF NOT EXISTS prover_fri_protocol_versions (
    id INTEGER NOT NULL PRIMARY KEY,
    recursion_scheduler_level_vk_hash BYTEA NOT NULL,
    recursion_node_level_vk_hash BYTEA NOT NULL,
    recursion_leaf_level_vk_hash BYTEA NOT NULL,
    recursion_circuits_set_vks_hash BYTEA NOT NULL,
    created_at TIMESTAMP NOT NULL
);

CREATE TABLE IF NOT EXISTS gpu_prover_queue (
    id BIGSERIAL PRIMARY KEY,
    instance_host INET NOT NULL,
    instance_port INTEGER NOT NULL CONSTRAINT valid_port CHECK (
        (instance_port >= 0)
        AND (instance_port <= 65535)
    ),
    instance_status TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    queue_free_slots INTEGER,
    queue_capacity INTEGER,
    specialized_prover_group_id SMALLINT,
    region TEXT NOT NULL,
    zone TEXT NOT NULL,
    num_gpu SMALLINT,
    UNIQUE (instance_host, instance_port, region, zone)
);

CREATE INDEX IF NOT EXISTS gpu_prover_queue_zone_region_idx ON gpu_prover_queue (region, zone);

CREATE TABLE IF NOT EXISTS gpu_prover_queue_fri (
    id BIGSERIAL PRIMARY KEY,
    instance_host INET NOT NULL,
    instance_port INTEGER NOT NULL CONSTRAINT valid_port CHECK (
        (instance_port >= 0)
        AND (instance_port <= 65535)
    ),
    instance_status TEXT NOT NULL,
    specialized_prover_group_id SMALLINT NOT NULL,
    zone TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP
);

CREATE UNIQUE INDEX gpu_prover_queue_fri_host_port_zone_idx ON gpu_prover_queue_fri (instance_host, instance_port, zone);


CREATE TABLE IF NOT EXISTS leaf_aggregation_witness_jobs (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    basic_circuits BYTEA NOT NULL,
    basic_circuits_inputs BYTEA NOT NULL,
    number_of_basic_circuits INTEGER NOT NULL,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    attempts INTEGER DEFAULT 0 NOT NULL,
    basic_circuits_blob_url TEXT,
    basic_circuits_inputs_blob_url TEXT,
    is_blob_cleaned BOOLEAN DEFAULT false NOT NULL,
    protocol_version INTEGER CONSTRAINT leaf_aggregation_witness_jobs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
);

CREATE INDEX IF NOT EXISTS leaf_aggregation_witness_jobs_blob_cleanup_status_index ON leaf_aggregation_witness_jobs (status, is_blob_cleaned)


create TABLE IF NOT EXISTS leaf_aggregation_witness_jobs_fri (
    id BIGSERIAL PRIMARY KEY,
    l1_batch_number BIGINT NOT NULL,
    circuit_id SMALLINT NOT NULL,
    closed_form_inputs_blob_url TEXT,
    attempts SMALLINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    is_blob_cleaned BOOLEAN,
    number_of_basic_circuits INTEGER,
    protocol_version INTEGER REFERENCES prover_fri_protocol_versions,
    picked_by TEXT
);

CREATE UNIQUE INDEX leaf_aggregation_witness_jobs_fri_composite_index ON leaf_aggregation_witness_jobs_fri (l1_batch_number, circuit_id);

CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_witness_jobs_fri_queued_order ON leaf_aggregation_witness_jobs_fri (l1_batch_number, id)
WHERE
    (status = 'queued' :: TEXT);

CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_fri_status_processing_attempts ON leaf_aggregation_witness_jobs_fri (processing_started_at, attempts)
WHERE
    (
        status = ANY (ARRAY ['in_progress'::TEXT, 'failed'::TEXT])
    );


CREATE TABLE IF NOT EXISTS node_aggregation_witness_jobs (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    leaf_layer_subqueues BYTEA,
    aggregation_outputs BYTEA,
    number_of_leaf_circuits INTEGER,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    attempts INTEGER DEFAULT 0 NOT NULL,
    leaf_layer_subqueues_blob_url TEXT,
    aggregation_outputs_blob_url TEXT,
    is_blob_cleaned BOOLEAN DEFAULT false NOT NULL,
    protocol_version INTEGER CONSTRAINT node_aggregation_witness_jobs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
);

CREATE INDEX IF NOT EXISTS node_aggregation_witness_jobs_blob_cleanup_status_index ON node_aggregation_witness_jobs (status, is_blob_cleaned);


CREATE TABLE IF NOT EXISTS node_aggregation_witness_jobs_fri (
    id BIGSERIAL PRIMARY KEY,
    l1_batch_number BIGINT NOT NULL,
    circuit_id SMALLINT NOT NULL,
    depth INTEGER DEFAULT 0 NOT NULL,
    status TEXT NOT NULL,
    attempts SMALLINT DEFAULT 0 NOT NULL,
    aggregations_url TEXT,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    number_of_dependent_jobs INTEGER,
    protocol_version INTEGER REFERENCES prover_fri_protocol_versions,
    picked_by TEXT
);

CREATE UNIQUE INDEX node_aggregation_witness_jobs_fri_composite_index ON node_aggregation_witness_jobs_fri (l1_batch_number, circuit_id, depth);

CREATE INDEX IF NOT EXISTS idx_node_aggregation_witness_jobs_fri_queued_order ON node_aggregation_witness_jobs_fri (l1_batch_number, depth, id)
WHERE
    (status = 'queued' :: TEXT);

CREATE INDEX IF NOT EXISTS idx_node_aggregation_fri_status_processing_attempts ON node_aggregation_witness_jobs_fri (processing_started_at, attempts)
WHERE
    (
        status = ANY (ARRAY ['in_progress'::TEXT, 'failed'::TEXT])
    );


CREATE TABLE IF NOT EXISTS proof_compression_jobs_fri (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    attempts SMALLINT DEFAULT 0 NOT NULL,
    status TEXT NOT NULL,
    fri_proof_blob_url TEXT,
    l1_proof_blob_url TEXT,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    picked_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_proof_compression_jobs_fri_status_processing_attempts ON proof_compression_jobs_fri (processing_started_at, attempts)
WHERE
    (
        status = ANY (ARRAY ['in_progress'::TEXT, 'failed'::TEXT])
    );

CREATE INDEX IF NOT EXISTS idx_proof_compressor_jobs_fri_queued_order ON proof_compression_jobs_fri (l1_batch_number)
WHERE
    (status = 'queued' :: TEXT);

CREATE TABLE IF NOT EXISTS prover_jobs (
    id BIGSERIAL PRIMARY KEY,
    l1_batch_number BIGINT NOT NULL,
    circuit_type TEXT NOT NULL,
    prover_input BYTEA NOT NULL,
    status TEXT NOT NULL,
    error TEXT,
    processing_started_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    time_taken TIME DEFAULT '00:00:00' :: TIME WITHOUT TIME ZONE NOT NULL,
    aggregation_round INTEGER DEFAULT 0 NOT NULL,
    result BYTEA,
    sequence_number INTEGER DEFAULT 0 NOT NULL,
    attempts INTEGER DEFAULT 0 NOT NULL,
    circuit_input_blob_url TEXT,
    proccesed_by TEXT,
    is_blob_cleaned BOOLEAN DEFAULT FALSE NOT NULL,
    protocol_version INTEGER CONSTRAINT prover_jobs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
) WITH (
    autovacuum_vacuum_scale_factor = 0,
    autovacuum_vacuum_threshold = 50000
);

CREATE UNIQUE INDEX prover_jobs_composite_index ON prover_jobs (
    l1_batch_number,
    aggregation_round,
    sequence_number
);

CREATE INDEX IF NOT EXISTS prover_jobs_blob_cleanup_status_index ON prover_jobs (status, is_blob_cleaned);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_t1 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        status = ANY (
            ARRAY ['queued'::TEXT, 'in_progress'::TEXT, 'failed'::TEXT]
        )
    );

CREATE INDEX IF NOT EXISTS prover_jobs_circuit_type_and_status_index ON prover_jobs (circuit_type, status);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_t2 ON prover_jobs (l1_batch_number, aggregation_round)
WHERE
    (
        (status = 'successful' :: TEXT)
        OR (aggregation_round < 3)
    );

CREATE INDEX IF NOT EXISTS ix_prover_jobs_t3 ON prover_jobs (aggregation_round, l1_batch_number)
WHERE
    (status = 'successful' :: TEXT);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_0_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY ('{Scheduler,"L1 messages merklizer"}' :: TEXT [])
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_1_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY (
                '{"Node aggregation","Decommitts sorter"}' :: TEXT []
            )
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_2_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY (
                '{"Leaf aggregation","Code decommitter"}' :: TEXT []
            )
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_3_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY ('{"Log demuxer",Keccak}' :: TEXT [])
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_4_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY ('{SHA256,ECRecover}' :: TEXT [])
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_5_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY ('{"RAM permutation","Storage sorter"}' :: TEXT [])
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_6_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY (
                '{"Storage application","Initial writes pubdata rehasher"}' :: TEXT []
            )
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_7_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY (
                '{"Repeated writes pubdata rehasher","Events sorter"}' :: TEXT []
            )
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_8_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (
            circuit_type = ANY (
                '{"L1 messages sorter","L1 messages rehasher"}' :: TEXT []
            )
        )
    ) INCLUDE (protocol_version);

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_9_2 ON prover_jobs (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (
        (status = 'queued' :: TEXT)
        AND (circuit_type = ANY ('{"Main VM"}' :: TEXT []))
    ) INCLUDE (protocol_version);


CREATE TABLE IF NOT EXISTS prover_jobs_fri (
    id BIGSERIAL PRIMARY KEY,
    l1_batch_number BIGINT NOT NULL,
    circuit_id SMALLINT NOT NULL,
    circuit_blob_url TEXT NOT NULL,
    aggregation_round SMALLINT NOT NULL,
    sequence_number INTEGER NOT NULL,
    status TEXT NOT NULL,
    error TEXT,
    attempts SMALLINT DEFAULT 0 NOT NULL,
    processing_started_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    time_taken TIME,
    is_blob_cleaned BOOLEAN,
    depth INTEGER DEFAULT 0 NOT NULL,
    is_node_final_proof BOOLEAN DEFAULT FALSE NOT NULL,
    proof_blob_url TEXT,
    protocol_version INTEGER REFERENCES prover_fri_protocol_versions,
    picked_by TEXT
);

CREATE UNIQUE INDEX prover_jobs_fri_composite_index ON prover_jobs_fri (
    l1_batch_number,
    aggregation_round,
    circuit_id,
    depth,
    sequence_number
);

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order ON prover_jobs_fri (
    aggregation_round desc,
    l1_batch_number asc,
    id asc
)
WHERE
    (status = 'queued' :: TEXT);

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_status_processing_attempts ON prover_jobs_fri (processing_started_at, attempts)
WHERE
    (
        status = ANY (ARRAY ['in_progress'::TEXT, 'failed'::TEXT])
    );


CREATE TABLE IF NOT EXISTS prover_protocol_versions (
    id INTEGER NOT NULL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    recursion_scheduler_level_vk_hash BYTEA NOT NULL,
    recursion_node_level_vk_hash BYTEA NOT NULL,
    recursion_leaf_level_vk_hash BYTEA NOT NULL,
    recursion_circuits_set_vks_hash BYTEA NOT NULL,
    verifier_address BYTEA NOT NULL,
    created_at TIMESTAMP NOT NULL
);


CREATE TABLE IF NOT EXISTS scheduler_dependency_tracker_fri (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    status TEXT NOT NULL,
    circuit_1_final_prover_job_id BIGINT,
    circuit_2_final_prover_job_id BIGINT,
    circuit_3_final_prover_job_id BIGINT,
    circuit_4_final_prover_job_id BIGINT,
    circuit_5_final_prover_job_id BIGINT,
    circuit_6_final_prover_job_id BIGINT,
    circuit_7_final_prover_job_id BIGINT,
    circuit_8_final_prover_job_id BIGINT,
    circuit_9_final_prover_job_id BIGINT,
    circuit_10_final_prover_job_id BIGINT,
    circuit_11_final_prover_job_id BIGINT,
    circuit_12_final_prover_job_id BIGINT,
    circuit_13_final_prover_job_id BIGINT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scheduler_dependency_tracker_fri_circuit_ids_filtered ON scheduler_dependency_tracker_fri (
    circuit_1_final_prover_job_id,
    circuit_2_final_prover_job_id,
    circuit_3_final_prover_job_id,
    circuit_4_final_prover_job_id,
    circuit_5_final_prover_job_id,
    circuit_6_final_prover_job_id,
    circuit_7_final_prover_job_id,
    circuit_8_final_prover_job_id,
    circuit_9_final_prover_job_id,
    circuit_10_final_prover_job_id,
    circuit_11_final_prover_job_id,
    circuit_12_final_prover_job_id,
    circuit_13_final_prover_job_id
)
WHERE
    (status <> 'queued' :: TEXT);


CREATE TABLE IF NOT EXISTS scheduler_witness_jobs (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    scheduler_witness BYTEA NOT NULL,
    final_node_aggregations BYTEA,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    attempts INTEGER DEFAULT 0 NOT NULL,
    aggregation_result_coords BYTEA,
    scheduler_witness_blob_url TEXT,
    final_node_aggregations_blob_url TEXT,
    is_blob_cleaned BOOLEAN DEFAULT FALSE NOT NULL,
    protocol_version INTEGER CONSTRAINT scheduler_witness_jobs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
);

CREATE INDEX IF NOT EXISTS scheduler_witness_jobs_blob_cleanup_status_index ON scheduler_witness_jobs (status, is_blob_cleaned);


CREATE TABLE IF NOT EXISTS scheduler_witness_jobs_fri (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    scheduler_partial_input_blob_url TEXT NOT NULL,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    attempts SMALLINT DEFAULT 0 NOT NULL,
    protocol_version INTEGER REFERENCES prover_fri_protocol_versions,
    picked_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_scheduler_fri_status_processing_attempts ON scheduler_witness_jobs_fri (processing_started_at, attempts)
WHERE
    (
        status = ANY (ARRAY ['in_progress'::TEXT, 'failed'::TEXT])
    );


CREATE TABLE IF NOT EXISTS witness_inputs (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    merkle_tree_paths BYTEA,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    status TEXT NOT NULL,
    time_taken TIME DEFAULT '00:00:00' :: TIME WITHOUT TIME ZONE NOT NULL,
    processing_started_at TIMESTAMP,
    error varchar,
    attempts INTEGER DEFAULT 0 NOT NULL,
    merkel_tree_paths_blob_url TEXT,
    is_blob_cleaned BOOLEAN DEFAULT false NOT NULL,
    protocol_version INTEGER CONSTRAINT witness_inputs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
);

CREATE INDEX IF NOT EXISTS witness_inputs_blob_cleanup_status_index ON witness_inputs (status, is_blob_cleaned);


CREATE TABLE IF NOT EXISTS witness_inputs_fri (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    merkle_tree_paths_blob_url TEXT,
    attempts SMALLINT DEFAULT 0 NOT NULL,
    status TEXT NOT NULL,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    is_blob_cleaned BOOLEAN,
    protocol_version INTEGER REFERENCES prover_fri_protocol_versions,
    picked_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_status_processing_attempts ON witness_inputs_fri (processing_started_at, attempts)
WHERE
    (
        status = ANY (ARRAY ['in_progress'::TEXT, 'failed'::TEXT])
    );

CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_queued_order ON witness_inputs_fri (l1_batch_number)
WHERE
    (status = 'queued' :: TEXT);
