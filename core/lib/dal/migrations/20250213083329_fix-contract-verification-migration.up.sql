-- This migration fixes data that was corrupted during migration of rows from
-- `contracts_verification_info` to `contract_verification_info_v2`.
-- New rows in the table are not affected, so in case anything goes wrong, the old
-- data can still be recovered from `contracts_verification_info`.

-- What went wrong: during `COPY FROM STDIN`, the bytea values were not properly escaped,
-- so they got copied as text strings and each byte was represented as two characters.
-- E.g. `\xd8792c840591d437dac9bbdc6c8d2096b41800c0` was turned into
-- `\x5c7864383739326338343035393164343337646163396262646336633864323039366234313830306330`

-- What we're doing here:
-- 1. Select affected rows only
-- 2. Encode bytea values for them to become text (note that it adds additional `\` to the beginning of the string)
-- 3. Remove `\` from the beginning of the string
-- 4. Cast to `bytea` which now will be correctly parsed by postgres.
-- We also need to check if there are no duplicates in case the same contract was re-verified after the data was
-- corrupted but before it was fixed.
-- After migration, we remove entries that weren't migrated (since these are guaranteed to be duplicates).

UPDATE contract_verification_info_v2 AS t SET
  initial_contract_addr = substring(encode(t.initial_contract_addr, 'escape') from 2)::bytea,
  bytecode_keccak256 = substring(encode(t.bytecode_keccak256, 'escape') from 2)::bytea,
  bytecode_without_metadata_keccak256 = substring(encode(t.bytecode_without_metadata_keccak256, 'escape') from 2)::bytea
WHERE
 length(t.initial_contract_addr::text) > 42 -- 20 bytes address + 2 bytes \x
AND NOT EXISTS (
    SELECT 1 FROM contract_verification_info_v2 AS c
    WHERE c.initial_contract_addr = substring(encode(t.initial_contract_addr, 'escape') from 2)::bytea
);

-- Remove duplicates.
DELETE FROM contract_verification_info_v2
WHERE length(initial_contract_addr::text) > 42;
