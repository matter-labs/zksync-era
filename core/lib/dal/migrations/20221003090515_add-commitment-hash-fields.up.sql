ALTER TABLE l1_batches ADD COLUMN aux_data_hash BYTEA;
ALTER TABLE l1_batches ADD COLUMN pass_through_data_hash BYTEA;
ALTER TABLE l1_batches ADD COLUMN meta_parameters_hash BYTEA;
