ALTER TABLE l1_batches_consensus
ADD committee JSONB NOT NULL;

ALTER TABLE l1_batches_consensus
ALTER COLUMN certificate DROP NOT NULL;

--ALTER TABLE l1_batches_consensus DROP CONSTRAINT l1_batches_consensus_certificate_check;
--
--ALTER TABLE l1_batches_consensus ADD CONSTRAINT l1_batches_consensus_certificate_check
--CHECK (certificate IS NULL OR ((certificate->'message'->>'number')::numeric = l1_batch_number));
--
