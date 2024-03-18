CREATE INDEX IF NOT EXISTS events_tx_initiator_address_idx
    ON events (tx_initiator_address);
CREATE INDEX IF NOT EXISTS transactions_contract_address_idx
    ON transactions (contract_address);
