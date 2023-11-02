ALTER TABLE miniblocks
    ADD COLUMN commit_qc BYTEA NULL;
ALTER TABLE miniblocks
    ADD COLUMN prev_consensus_block_hash BYTEA NULL;
