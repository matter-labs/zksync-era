CREATE TABLE IF NOT EXISTS zkos_proofs
(
    l2_block_number             BIGINT NOT NULL PRIMARY KEY,
    input_gen_time_taken        TIME,
    prover_input                BYTEA,

    proof_picked_at             TIMESTAMP,
    proof_picked_by             VARCHAR,
    prove_time_taken            TIME,
    proof                       BYTEA,
    attempts                    INT,

    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL
);
