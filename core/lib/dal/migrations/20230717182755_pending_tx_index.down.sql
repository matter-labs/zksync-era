DROP INDEX IF EXISTS pending_l1_batch_txs;
CREATE INDEX IF NOT EXISTS transactions_l1_batch_number_idx ON transactions USING btree (l1_batch_number)
