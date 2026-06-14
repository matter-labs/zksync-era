-- Track how many times a batch has been handed out for proving so a batch that keeps failing (or
-- times out) is retried only a bounded number of times instead of forever. `attempts` counts FRI
-- picks, `snark_attempts` counts SNARK picks. `error` stores the last failure reported by a prover.
ALTER TABLE airbender_proof_generation_details
    ADD COLUMN IF NOT EXISTS attempts SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS snark_attempts SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS error TEXT;
