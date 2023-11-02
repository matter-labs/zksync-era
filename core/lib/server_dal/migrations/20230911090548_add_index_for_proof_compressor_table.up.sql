-- Add up migration script here
CREATE INDEX IF NOT EXISTS idx_proof_compressor_jobs_fri_queued_order
    ON proof_compression_jobs_fri (l1_batch_number ASC)
    WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_queued_order
    ON witness_inputs_fri (l1_batch_number ASC)
    WHERE status = 'queued';
