DROP TABLE IF EXISTS gpu_prover_queue;

CREATE TABLE IF NOT EXISTS gpu_prover_queue
(
    id                          BIGSERIAL NOT NULL PRIMARY KEY,
    instance_host               INET      NOT NULL,
    instance_port               INT       NOT NULL
        CONSTRAINT valid_port CHECK (instance_port >= 0 AND instance_port <= 65535),
    instance_status             TEXT      NOT NULL,
    created_at                  TIMESTAMP NOT NULL,
    updated_at                  TIMESTAMP NOT NULL,
    processing_started_at       TIMESTAMP,
    queue_free_slots            integer,
    queue_capacity              integer,
    specialized_prover_group_id smallint,
    region                      TEXT      NOT NULL,
    zone                        TEXT      NOT NULL,
    num_gpu                     smallint,
    unique (instance_host, instance_port, region, zone)
);

CREATE INDEX IF NOT EXISTS gpu_prover_queue_zone_region_idx ON gpu_prover_queue (region, zone);
