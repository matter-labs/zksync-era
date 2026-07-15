-- Drop the new columns with their defaults and constraints
ALTER TABLE base_token_ratios DROP COLUMN IF EXISTS numerator_sl;
ALTER TABLE base_token_ratios DROP COLUMN IF EXISTS denominator_sl;

-- Rename columns back to original names
ALTER TABLE base_token_ratios RENAME COLUMN numerator_l1 TO numerator;
ALTER TABLE base_token_ratios RENAME COLUMN denominator_l1 TO denominator;
