UPDATE transactions
SET data = data || '{"to_porter_authorized": false}'::jsonb
WHERE type = 'deposit'
