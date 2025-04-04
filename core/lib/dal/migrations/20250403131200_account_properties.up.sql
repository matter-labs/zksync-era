CREATE TABLE account_properties (
    address BYTEA NOT NULL,
    miniblock_number BIGINT NOT NULL,

    preimage_hash BYTEA NOT NULL,

    versioning_data NUMERIC(20) NOT NULL,
    nonce NUMERIC(20) NOT NULL,
    observable_bytecode_hash BYTEA NOT NULL,
    bytecode_hash BYTEA NOT NULL,
    nominal_token_balance NUMERIC(80) NOT NULL,
    bytecode_len BIGINT NOT NULL,
    artifacts_len BIGINT NOT NULL,
    observable_bytecode_len BIGINT NOT NULL,

    PRIMARY KEY (address, miniblock_number)
);

CREATE INDEX account_properties_preimage_hash_idx ON account_properties (preimage_hash);
CREATE INDEX account_properties_miniblock_number_idx ON account_properties (miniblock_number);
