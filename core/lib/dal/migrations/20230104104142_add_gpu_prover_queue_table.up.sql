CREATE TABLE IF NOT EXISTS gpu_prover_queue
(
    instance_host         INET      NOT NULL,
    instance_port         INT       NOT NULL
        CONSTRAINT valid_port CHECK (instance_port >= 0 AND instance_port <= 65535),
    instance_status       TEXT      NOT NULL,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    PRIMARY KEY (instance_host, instance_port)
);
