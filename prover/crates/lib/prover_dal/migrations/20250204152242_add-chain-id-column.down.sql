ALTER TABLE witness_inputs_fri DROP CONSTRAINT IF EXISTS witness_inputs_fri_pkey;
ALTER TABLE recursion_tip_witness_jobs_fri DROP CONSTRAINT IF EXISTS recursion_tip_witness_jobs_fri_pkey;
ALTER TABLE scheduler_witness_jobs_fri DROP CONSTRAINT IF EXISTS scheduler_witness_jobs_fri_pkey;
ALTER TABLE proof_compression_jobs_fri DROP CONSTRAINT IF EXISTS proof_compression_jobs_fri_pkey;

ALTER TABLE witness_inputs_fri ADD CONSTRAINT witness_inputs_fri_pkey PRIMARY KEY (l1_batch_number);
ALTER TABLE recursion_tip_witness_jobs_fri ADD CONSTRAINT recursion_tip_witness_jobs_fri_pkey PRIMARY KEY (l1_batch_number);
ALTER TABLE scheduler_witness_jobs_fri ADD CONSTRAINT scheduler_witness_jobs_fri_pkey PRIMARY KEY (l1_batch_number);
ALTER TABLE proof_compression_jobs_fri ADD CONSTRAINT proof_compression_jobs_fri_pkey PRIMARY KEY (l1_batch_number);

ALTER TABLE witness_inputs_fri DROP COLUMN chain_id;
ALTER TABLE leaf_aggregation_witness_jobs_fri DROP COLUMN chain_id;
ALTER TABLE node_aggregation_witness_jobs_fri DROP COLUMN chain_id;
ALTER TABLE recursion_tip_witness_jobs_fri DROP COLUMN chain_id;
ALTER TABLE scheduler_witness_jobs_fri DROP COLUMN chain_id;
ALTER TABLE proof_compression_jobs_fri DROP COLUMN chain_id;
ALTER TABLE prover_jobs_fri DROP COLUMN chain_id;
ALTER TABLE prover_jobs_fri_archive DROP COLUMN chain_id;
