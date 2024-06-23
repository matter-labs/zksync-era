ALTER TABLE eth_txs ADD COLUMN created_at_block INTEGER;
UPDATE eth_txs SET created_at_block = (
    SELECT
       COALESCE(MAX(sent_at_block), 0)
    FROM
       eth_txs_history
    WHERE
       sent_at_block IS NOT NULL
);
ALTER TABLE eth_txs ALTER COLUMN created_at_block SET NOT NULL;
