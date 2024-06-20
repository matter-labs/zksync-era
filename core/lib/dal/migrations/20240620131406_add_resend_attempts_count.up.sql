ALTER TABLE eth_txs ADD COLUMN resend_attempts_count INT NOT NULL DEFAULT 0;
