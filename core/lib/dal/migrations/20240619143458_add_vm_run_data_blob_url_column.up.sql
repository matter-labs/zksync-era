ALTER TABLE proof_generation_details
 ADD COLUMN IF NOT EXISTS vm_run_data_blob_url DEFAULT NULL;