CREATE INDEX IF NOT EXISTS ix_events_t1 ON public.events USING btree (topic1, address, tx_hash);
CREATE INDEX IF NOT EXISTS ix_initial_writes_t1 ON public.initial_writes USING btree (hashed_key) INCLUDE (l1_batch_number);
CREATE INDEX IF NOT EXISTS ix_miniblocks_t1 ON public.miniblocks USING btree (number) INCLUDE (l1_batch_number, "timestamp");

CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_0 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{Scheduler,"L1 messages merklizer"}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_1 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{"Node aggregation","Decommitts sorter"}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_2 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{"Leaf aggregation","Code decommitter"}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_3 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{"Log demuxer",Keccak}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_4 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{SHA256,ECRecover}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_5 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{"RAM permutation","Storage sorter"}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_6 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{"Storage application","Initial writes pubdata rehasher"}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_7 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{"Repeated writes pubdata rehasher","Events sorter"}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_8 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{"L1 messages sorter","L1 messages rehasher"}'::text[])));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_circuits_9 ON public.prover_jobs USING btree (aggregation_round DESC, l1_batch_number, id) WHERE ((status = 'queued'::text) AND (circuit_type = ANY ('{"Main VM"}'::text[])));

CREATE INDEX IF NOT EXISTS ix_prover_jobs_t2 ON public.prover_jobs USING btree (l1_batch_number, aggregation_round) WHERE ((status = 'successful'::text) OR (aggregation_round < 3));
CREATE INDEX IF NOT EXISTS ix_prover_jobs_t3 ON public.prover_jobs USING btree (aggregation_round, l1_batch_number) WHERE (status = 'successful'::text);
