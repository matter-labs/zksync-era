ALTER TABLE gpu_prover_queue DROP CONSTRAINT IF EXISTS gpu_prover_queue_pkey;

ALTER TABLE gpu_prover_queue ADD CONSTRAINT gpu_prover_queue_pkey PRIMARY KEY (instance_host, instance_port, region);
