ALTER TABLE prover_jobs_fri ADD CONSTRAINT prover_jobs_fri_l1_batch_number_fkey
    FOREIGN KEY (l1_batch_number) REFERENCES l1_batches (number);
