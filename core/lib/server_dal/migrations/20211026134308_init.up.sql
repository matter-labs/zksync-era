CREATE TABLE storage
(
    raw_key BYTEA PRIMARY KEY,
    value   BYTEA NOT NULL,
    tx_hash BYTEA NOT NULL,
    address BYTEA NOT NULL,
    key     BYTEA NOT NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
)
    WITH (fillfactor = 50);

CREATE TABLE eth_txs (
    id SERIAL NOT NULL PRIMARY KEY ,
    nonce             BIGINT    NOT NULL,
    raw_tx            BYTEA     NOT NULL,
    contract_address  TEXT      NOT NULL,
    tx_type           TEXT      NOT NULL,
    gas_price         BIGINT,
    confirmed_tx_hash TEXT,
    confirmed_at      TIMESTAMP,
    gas_used          BIGINT,

    created_at        TIMESTAMP NOT NULL,
    updated_at        TIMESTAMP NOT NULL
);

CREATE TABLE eth_txs_history (
    id SERIAL NOT NULL PRIMARY KEY ,
    eth_tx_id SERIAL NOT NULL REFERENCES eth_txs (id) ON DELETE CASCADE,
    gas_price BIGINT NOT NULL ,
    deadline_block INT NOT NULL,
    tx_hash TEXT NOT NULL,
    error TEXT,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE TABLE blocks
(
    number BIGSERIAL PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    is_finished BOOL NOT NULL,
    priority_ops_complexity NUMERIC(80) NOT NULL,
    l1_tx_count INT NOT NULL,
    l2_tx_count INT NOT NULL,
    fee_account_address BYTEA NOT NULL,
    bloom BYTEA NOT NULL,
    priority_ops_onchain_data BYTEA[] NOT NULL,
    processable_onchain_ops BYTEA[] NOT NULL,

    hash BYTEA,
    parent_hash BYTEA,
    commitment BYTEA,
    compressed_write_logs BYTEA,
    compressed_contracts BYTEA,
    eth_prove_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL,
    eth_commit_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL,
    eth_execute_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX blocks_eth_commit_tx_id_idx ON blocks (eth_commit_tx_id);
CREATE INDEX blocks_eth_execute_tx_id_idx ON blocks (eth_execute_tx_id);
CREATE INDEX blocks_eth_prove_tx_id_idx ON blocks (eth_prove_tx_id);

CREATE TABLE transactions
(
    hash BYTEA PRIMARY KEY,
    is_priority BOOLEAN NOT NULL,
    full_fee NUMERIC(80),
    layer_2_tip_fee NUMERIC(80),
    initiator_address BYTEA NOT NULL,
    nonce BIGINT NOT NULL,
    signature BYTEA,
    valid_from NUMERIC(20) NOT NULL,
    valid_until NUMERIC(20) NOT NULL,
    fee_token BYTEA,
    input BYTEA,
    data JSONB NOT NULL,
    type VARCHAR NOT NULL,
    received_at TIMESTAMP NOT NULL,
    priority_op_id BIGINT,

    block_number BIGINT REFERENCES blocks (number) ON DELETE SET NULL,
    index_in_block INT,
    error VARCHAR,

    ergs_limit NUMERIC(80),
    ergs_price_limit NUMERIC(80),
    ergs_per_storage_limit NUMERIC(80),
    ergs_per_pubdata_limit NUMERIC(80),
    tx_format INTEGER,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX transactions_block_number_idx ON transactions (block_number);

CREATE TABLE aggregated_proof (
   id SERIAL PRIMARY KEY,
   from_block_number  BIGINT REFERENCES blocks (number) ON DELETE CASCADE,
   to_block_number  BIGINT REFERENCES blocks (number) ON DELETE CASCADE,
   proof BYTEA,
   eth_prove_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL,

   created_at TIMESTAMP NOT NULL
);

CREATE TABLE proof (
    block_number BIGINT PRIMARY KEY REFERENCES blocks (number) ON DELETE CASCADE,
    proof BYTEA,

    created_at TIMESTAMP NOT NULL
);

CREATE TABLE contracts
(
    address BYTEA PRIMARY KEY,
    bytecode BYTEA NOT NULL,
    tx_hash BYTEA NOT NULL,
    block_number BIGINT NOT NULL REFERENCES blocks (number) ON DELETE CASCADE,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX contracts_block_number_idx ON contracts (block_number);
CREATE INDEX contracts_tx_hash_idx ON contracts (tx_hash);

CREATE TABLE storage_logs
(
    id BIGSERIAL PRIMARY KEY,
    raw_key BYTEA NOT NULL,
    address BYTEA NOT NULL,
    key BYTEA NOT NULL,
    value BYTEA NOT NULL,
    operation_number INT NOT NULL,
    tx_hash BYTEA NOT NULL,
    block_number BIGINT NOT NULL REFERENCES blocks (number) ON DELETE CASCADE,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX storage_logs_block_number_idx ON storage_logs (block_number);
CREATE INDEX storage_logs_raw_key_block_number_idx ON storage_logs (raw_key, block_number DESC, operation_number DESC);

CREATE TABLE contract_sources
(
    address BYTEA PRIMARY KEY,
    assembly_code TEXT NOT NULL,
    pc_line_mapping JSONB NOT NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE TABLE transaction_traces
(
    tx_hash BYTEA PRIMARY KEY,
    trace JSONB NOT NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE TABLE events
(
    id BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL REFERENCES blocks (number) ON DELETE CASCADE,
    tx_hash BYTEA NOT NULL,
    tx_index_in_block INT NOT NULL,
    address BYTEA NOT NULL,

    event_index_in_block INT NOT NULL,
    event_index_in_tx INT NOT NULL,

    topic1 BYTEA NOT NULL,
    topic2 BYTEA NOT NULL,
    topic3 BYTEA NOT NULL,
    topic4 BYTEA NOT NULL,

    value BYTEA NOT NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX events_tx_location_idx ON events (block_number, tx_index_in_block);
CREATE INDEX events_address_idx ON events USING hash (address);
CREATE INDEX events_topic1_idx ON events USING hash (topic1);
CREATE INDEX events_topic2_idx ON events USING hash (topic2);
CREATE INDEX events_topic3_idx ON events USING hash (topic3);
CREATE INDEX events_topic4_idx ON events USING hash (topic4);
CREATE INDEX events_tx_hash_idx ON events USING hash (tx_hash);

CREATE TABLE factory_deps
(
    bytecode_hash BYTEA PRIMARY KEY,
    bytecode BYTEA NOT NULL,
    block_number BIGINT NOT NULL REFERENCES blocks (number) ON DELETE CASCADE,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE TABLE tokens (
    address BYTEA PRIMARY KEY,
    name VARCHAR NOT NULL,
    symbol VARCHAR NOT NULL,
    decimals INT NOT NULL,
    well_known BOOLEAN NOT NULL,

    token_list_name VARCHAR,
    token_list_symbol VARCHAR,
    token_list_decimals INT,

    usd_price NUMERIC,
    usd_price_updated_at TIMESTAMP,

    market_volume NUMERIC,
    market_volume_updated_at TIMESTAMP,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
)
