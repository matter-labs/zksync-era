CREATE TABLE base_token_ratios (
    id SERIAL PRIMARY KEY,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,

    ratio_timestamp TIMESTAMP NOT NULL,
    numerator NUMERIC(20,0) NOT NULL,
    denominator NUMERIC(20,0) NOT NULL,

    used_in_l1 BOOLEAN NOT NULL DEFAULT FALSE
);
