CREATE TABLE IF NOT EXISTS tee_dcap_collateral (
    kind                 VARCHAR(20) NOT NULL PRIMARY KEY,
    not_after            TIMESTAMPTZ NOT NULL,
    sha256 BYTEA NOT NULL,
    updated              TIMESTAMPTZ NOT NULL,
    calldata             BYTEA NOT NULL,
    eth_tx_id            INTEGER     REFERENCES eth_txs (id) ON DELETE SET NULL,
    update_guard_expires TIMESTAMPTZ
);
