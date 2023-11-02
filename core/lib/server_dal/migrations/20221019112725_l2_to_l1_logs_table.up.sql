CREATE TABLE l2_to_l1_logs (
   miniblock_number BIGINT NOT NULL REFERENCES miniblocks (number) ON DELETE CASCADE,
   log_index_in_miniblock INT NOT NULL,
   log_index_in_tx INT NOT NULL,

   tx_hash BYTEA NOT NULL,

   shard_id INT NOT NULL,
   is_service BOOLEAN NOT NULL,
   tx_index_in_miniblock INT NOT NULL,
   tx_index_in_l1_batch INT NOT NULL,
   sender BYTEA NOT NULL,
   key BYTEA NOT NULL,
   value BYTEA NOT NULL,

   created_at TIMESTAMP NOT NULL,
   updated_at TIMESTAMP NOT NULL,

   PRIMARY KEY (miniblock_number, log_index_in_miniblock)
);

CREATE INDEX l2_to_l1_logs_tx_hash_index ON l2_to_l1_logs USING hash (tx_hash);
