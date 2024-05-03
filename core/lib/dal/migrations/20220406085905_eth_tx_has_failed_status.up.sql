ALTER TABLE eth_txs ADD COLUMN has_failed BOOLEAN NOT NULL default false;
CREATE INDEX eth_txs_has_failed_idx ON eth_txs(has_failed) WHERE has_failed = TRUE;
