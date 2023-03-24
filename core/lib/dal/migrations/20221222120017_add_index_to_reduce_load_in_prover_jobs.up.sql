create index if not exists ix_prover_jobs_t1 on prover_jobs (aggregation_round DESC, l1_batch_number ASC, id ASC) where status in ('queued', 'in_progress', 'failed');
