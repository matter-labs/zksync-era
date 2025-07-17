CREATE INDEX IF NOT EXISTS prover_jobs_fri_get_next_job_index 
    ON prover_jobs_fri USING btree ( 
        protocol_version ASC, 
        protocol_version_patch ASC, 
        priority DESC, 
        batch_sealed_at ASC, 
        aggregation_round ASC, 
        circuit_id ASC, 
        id ASC 
    ) 
    INCLUDE ( chain_id ) 
    WHERE ( status = 'queued'::text );