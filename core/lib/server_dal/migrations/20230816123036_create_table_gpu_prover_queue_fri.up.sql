-- Add up migration script here
CREATE TABLE IF NOT EXISTS gpu_prover_queue_fri
(
    id                          BIGSERIAL PRIMARY KEY,
    instance_host               INET      NOT NULL,
    instance_port               INT       NOT NULL
        CONSTRAINT valid_port CHECK (instance_port >= 0 AND instance_port <= 65535),
    instance_status             TEXT      NOT NULL,
    specialized_prover_group_id SMALLINT  NOT NULL,
    zone                        TEXT,
    created_at                  TIMESTAMP NOT NULL,
    updated_at                  TIMESTAMP NOT NULL,
    processing_started_at       TIMESTAMP
);
