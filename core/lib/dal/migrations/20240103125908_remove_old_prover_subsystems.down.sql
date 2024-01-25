-- Note that era can't revert to this point in time.
-- These tables are added only if engineers want to revert from a future codebase to a previous codebase.
-- This migration will enable backwards development (i.e. bisecting some error).

CREATE TABLE IF NOT EXISTS gpu_prover_queue (
    id bigint NOT NULL PRIMARY KEY,
    instance_host inet NOT NULL,
    instance_port integer NOT NULL,
    instance_status text NOT NULL,
    created_at timestamp without time zone NOT NULL,
    updated_at timestamp without time zone NOT NULL,
    processing_started_at timestamp without time zone,
    queue_free_slots integer,
    queue_capacity integer,
    specialized_prover_group_id smallint,
    region text NOT NULL,
    zone text NOT NULL,
    num_gpu smallint,
    CONSTRAINT valid_port CHECK (((instance_port >= 0) AND (instance_port <= 65535)))
);

CREATE TABLE IF NOT EXISTS prover_jobs (
    id bigint NOT NULL PRIMARY KEY,
    l1_batch_number bigint NOT NULL,
    circuit_type text NOT NULL,
    prover_input bytea NOT NULL,
    status text NOT NULL,
    error text,
    processing_started_at timestamp without time zone,
    created_at timestamp without time zone NOT NULL,
    updated_at timestamp without time zone NOT NULL,
    time_taken time without time zone DEFAULT '00:00:00'::time without time zone NOT NULL,
    aggregation_round integer DEFAULT 0 NOT NULL,
    result bytea,
    sequence_number integer DEFAULT 0 NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    circuit_input_blob_url text,
    proccesed_by text,
    is_blob_cleaned boolean DEFAULT false NOT NULL,
    protocol_version integer
);

CREATE TABLE IF NOT EXISTS prover_protocol_versions (
    id integer NOT NULL,
    "timestamp" bigint NOT NULL,
    recursion_scheduler_level_vk_hash bytea NOT NULL,
    recursion_node_level_vk_hash bytea NOT NULL,
    recursion_leaf_level_vk_hash bytea NOT NULL,
    recursion_circuits_set_vks_hash bytea NOT NULL,
    verifier_address bytea NOT NULL,
    created_at timestamp without time zone NOT NULL
);
