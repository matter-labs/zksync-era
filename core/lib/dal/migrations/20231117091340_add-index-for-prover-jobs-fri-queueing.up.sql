CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order2 ON public.prover_jobs_fri USING btree (l1_batch_number, aggregation_round DESC, id) WHERE (status = 'queued'::text)
