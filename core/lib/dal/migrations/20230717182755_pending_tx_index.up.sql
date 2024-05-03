CREATE INDEX IF NOT EXISTS pending_l1_batch_txs ON public.transactions USING btree (miniblock_number, index_in_block) WHERE ((miniblock_number IS NOT NULL) AND (l1_batch_number IS NULL));
DROP INDEX IF EXISTS transactions_l1_batch_number_idx;
