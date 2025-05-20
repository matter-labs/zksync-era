CREATE TABLE IF NOT EXISTS tee_dcap_collateral_certs (
    serial_number BYTEA NOT NULL,
    issuer TEXT NOT NULL,
    sha256 BYTEA NOT NULL,
    not_after TIMESTAMPTZ NOT NULL,
    updated TIMESTAMPTZ NOT NULL,
    update_guard_expires TIMESTAMPTZ,
    PRIMARY KEY (serial_number, issuer)
);

CREATE TYPE tee_dcap_collateral_kind AS ENUM (
    'root_crl',
    'pck_crl',
    'sgx_tcb_info_json',
    'tdx_tcb_info_json',
    'sgx_qe_identity_json',
    'tdx_qe_identity_json'
);
CREATE TABLE IF NOT EXISTS tee_dcap_collateral (
    kind tee_dcap_collateral_kind PRIMARY KEY,
    not_after TIMESTAMPTZ NOT NULL,
    sha256 BYTEA NOT NULL,
    updated TIMESTAMPTZ NOT NULL,
    update_guard_expires TIMESTAMPTZ
);

CREATE TABLE chain_update_delay (
    id INTEGER DEFAULT(1) PRIMARY KEY,
    delay INTERVAL NOT NULL
);
INSERT INTO chain_update_delay (id, delay) VALUES (1, '1 hour');
