CREATE TYPE tee_dcap_collateral_kind AS ENUM (
    'root_ca',
    'root_crl',
    'pck_ca',
    'pck_crl',
    'sgx_qe_identity_json',
    'tdx_qe_identity_json'
);

CREATE TABLE IF NOT EXISTS tee_dcap_collateral (
    kind tee_dcap_collateral_kind PRIMARY KEY,
    not_after TIMESTAMPTZ NOT NULL,
    sha256 BYTEA NOT NULL,
    updated TIMESTAMPTZ NOT NULL,
    update_guard_set TIMESTAMPTZ,
    eth_tx_id INTEGER REFERENCES eth_txs (id) ON DELETE SET NULL
);

CREATE TYPE tee_dcap_collateral_tcb_info_json_kind AS ENUM (
    'sgx_tcb_info_json',
    'tdx_tcb_info_json'
);

CREATE TABLE IF NOT EXISTS tee_dcap_collateral_tcb_info_json (
    kind tee_dcap_collateral_tcb_info_json_kind NOT NULL,
    fmspc BYTEA NOT NULL,
    not_after TIMESTAMPTZ NOT NULL,
    sha256 BYTEA NOT NULL,
    updated TIMESTAMPTZ NOT NULL,
    update_guard_set TIMESTAMPTZ,
    eth_tx_id INTEGER REFERENCES eth_txs (id) ON DELETE SET NULL,
    PRIMARY KEY (kind, fmspc)
);
