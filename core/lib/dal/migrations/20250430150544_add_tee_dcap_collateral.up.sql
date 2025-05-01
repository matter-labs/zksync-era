CREATE TABLE IF NOT EXISTS tee_dcap_collateral_certs (
    serial_number BYTEA NOT NULL,
    issuer TEXT NOT NULL,
    sha256 BYTEA NOT NULL,
    not_after TIMESTAMP NOT NULL,
    updated TIMESTAMP NOT NULL,
    PRIMARY KEY (serial_number, issuer)
);

CREATE TYPE tee_dcap_collateral_kind AS ENUM ('root_crl', 'pck_crl', 'tcb_info_json', 'qe_identity_json');
CREATE TABLE IF NOT EXISTS tee_dcap_collateral (
    kind tee_dcap_collateral_kind PRIMARY KEY,
    not_after TIMESTAMP NOT NULL,
    sha256 BYTEA NOT NULL,
    updated TIMESTAMP NOT NULL
);

/*


SELECT kind FROM tee_dcap_collateral WHERE kind = $1 AND sha256 = $2 AND transaction_timestamp() < not_after;

INSERT INTO tee_dcap_collateral_certs VALUES ($1, $2, $3, $4, transaction_timestamp())
ON CONFLICT (serial_number, issuer) DO UPDATE SET sha256 = $3, not_after = $4, updated = transaction_timestamp();

INSERT INTO tee_dcap_collateral VALUES ($1, $2, $3, transaction_timestamp()) ON CONFLICT (kind) DO UPDATE SET sha256 = $2, not_after = $3;
*/