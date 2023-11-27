CREATE TABLE snapshots
(
    l1_batch_number          BIGINT    NOT NULL PRIMARY KEY,
    storage_logs_filepaths   TEXT[]    NOT NULL,
    factory_deps_filepath    TEXT      NOT NULL,

    created_at               TIMESTAMP NOT NULL,
    updated_at               TIMESTAMP NOT NULL
);
