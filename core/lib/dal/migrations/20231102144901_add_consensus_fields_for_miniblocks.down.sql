ALTER TABLE miniblocks
    DROP COLUMN IF EXISTS commit_qc;
ALTER TABLE miniblocks
    DROP COLUMN IF EXISTS prev_consensus_block_hash;
