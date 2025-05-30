CREATE TABLE IF NOT EXISTS zkos_proofs
(
    l2_block_number             BIGINT NOT NULL PRIMARY KEY,
    input_gen_time_taken        TIME,
    prover_input                BYTEA,

    fri_proof_picked_at             TIMESTAMP,
    fri_proof_picked_by             VARCHAR,
    fri_prove_time_taken            TIME,
    fri_proof                       BYTEA,
    fri_proof_attempts                    INT,

    snark_proof_picked_at             TIMESTAMP,
    snark_proof_picked_by             VARCHAR,
    snark_prove_time_taken            TIME,
    snark_proof                       BYTEA,
    snark_proof_attempts                    INT,

    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL
);
