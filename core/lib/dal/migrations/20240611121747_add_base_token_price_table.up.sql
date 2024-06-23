CREATE TABLE base_token_prices (
    id SERIAL PRIMARY KEY,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,

    ratio_timestamp TIMESTAMP NOT NULL,
    base_token_price NUMERIC NOT NULL,
    eth_price NUMERIC NOT NULL,

    used_in_l1 BOOLEAN NOT NULL DEFAULT FALSE
);
