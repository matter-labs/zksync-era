CREATE TABLE IF NOT EXISTS token_price_ratio (
    token_address BYTEA NOT NULL,
    ratio TEXT NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    PRIMARY KEY (token_address)
);
