-- Rename existing columns and add new columns
ALTER TABLE base_token_ratios RENAME COLUMN numerator TO numerator_l1;
ALTER TABLE base_token_ratios RENAME COLUMN denominator TO denominator_l1;

ALTER TABLE base_token_ratios ADD COLUMN numerator_sl NUMERIC(20,0) DEFAULT 1 NOT NULL;
ALTER TABLE base_token_ratios ADD COLUMN denominator_sl NUMERIC(20,0) DEFAULT 1 NOT NULL;
