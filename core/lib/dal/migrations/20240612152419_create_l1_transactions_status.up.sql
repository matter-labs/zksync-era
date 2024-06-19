CREATE TYPE transaction_status AS ENUM ('Created', 'Pending', 'Confirmed', 'Failed');
CREATE TYPE transaction_type AS ENUM ('Commit', 'PublishProofOnchain', 'Execute');

ALTER TABLE eth_txs ADD COLUMN idempotency_key VARCHAR UNIQUE;

CREATE TABLE l1_transactions
(
    id               SERIAL             NOT NULL PRIMARY KEY,
    raw_tx           BYTEA              NOT NULL,
    contract_address TEXT               NOT NULL,
    blob_sidecar     BYTEA,
    tx_hash          TEXT,
    tx_type          transaction_type   NOT NULL,
    status           transaction_status NOT NULL,

    created_at       TIMESTAMP          NOT NULL,
    updated_at       TIMESTAMP          NOT NULL
);

-- for some time, we will need to fetch both old and new transactions
ALTER TABLE zksync.public.l1_batches ADD column l1_commit_transaction_id INT REFERENCES l1_transactions(id) ON DELETE SET NULL;
ALTER TABLE zksync.public.l1_batches ADD column l1_prove_transaction_id INT REFERENCES l1_transactions(id) ON DELETE SET NULL;
ALTER TABLE zksync.public.l1_batches ADD column l1_execute_transaction_id INT REFERENCES l1_transactions(id) ON DELETE SET NULL;

