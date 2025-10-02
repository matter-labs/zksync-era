ALTER TABLE eth_txs_history
    ADD COLUMN sent_successfully BOOLEAN NOT NULL DEFAULT TRUE;
