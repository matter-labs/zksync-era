CREATE INDEX IF NOT EXISTS transactions_pending_mempool_idx
ON transactions (is_priority DESC, priority_op_id, received_at)
WHERE
    miniblock_number IS NULL
    AND in_mempool = FALSE
    AND error IS NULL;
