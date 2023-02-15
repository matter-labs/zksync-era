CREATE TABLE static_artifact_storage
(
    key VARCHAR NOT NULL PRIMARY KEY,
    value BYTEA NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);