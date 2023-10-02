ALTER TABLE witness_inputs_fri ADD CONSTRAINT witness_inputs_fri_l1_batch_number_fkey
    FOREIGN KEY (l1_batch_number) REFERENCES l1_batches (number);