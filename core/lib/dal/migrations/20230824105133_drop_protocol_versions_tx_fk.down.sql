ALTER TABLE protocol_versions ADD CONSTRAINT protocol_versions_upgrade_tx_hash_fkey
    FOREIGN KEY (upgrade_tx_hash) REFERENCES transactions (hash);
