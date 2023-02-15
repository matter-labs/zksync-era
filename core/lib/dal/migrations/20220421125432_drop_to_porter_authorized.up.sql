UPDATE transactions
SET data = data - 'to_porter_authorized'
WHERE type = 'deposit'
