ALTER TABLE gpu_prover_queue ADD CONSTRAINT gpu_prover_unique_idx UNIQUE(instance_host, instance_port, region, zone);
