-- Your SQL goes here
CREATE TABLE oracle
(
    id SERIAL NOT NULL PRIMARY KEY ,
    gas_token_adjust_coefficient NUMERIC NOT NULL DEFAULT 1.0,
    gas_token_adjust_coefficient_updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO oracle VALUES (1, DEFAULT, DEFAULT);