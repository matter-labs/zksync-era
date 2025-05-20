-- Serves as "registration" hub for provers. WVGs query this table to know which provers are available and how to reach them.
CREATE TABLE IF NOT EXISTS gpu_prover_queue_fri (
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
    protocol_version            INT,
    protocol_version_patch      INT NOT NULL DEFAULT 0
);

COMMENT ON TABLE gpu_prover_queue_fri IS 'Serves as "registration" hub for provers. WVGs query this table to know which provers are available and how to reach them.';

CREATE UNIQUE INDEX IF NOT EXISTS gpu_prover_queue_fri_host_port_zone_idx
    ON gpu_prover_queue_fri (instance_host, instance_port, zone);
-- Not clear if this index is used or necessary, but as part of the effort to backport everything from mainnet, it is included.
CREATE INDEX IF NOT EXISTS tmp_gpu_prover_queue_fri_t2
    ON gpu_prover_queue_fri (specialized_prover_group_id, zone, updated_at, instance_status, processing_started_at)
    INCLUDE (id);
