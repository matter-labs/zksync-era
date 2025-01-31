ALTER TABLE miniblocks DROP COLUMN consensus;

CREATE TABLE miniblocks_consensus (
  number BIGINT NOT NULL,
  certificate JSONB NOT NULL,
  PRIMARY KEY(number),
  CHECK((certificate->'message'->'proposal'->'number')::jsonb::numeric = number),
  CONSTRAINT miniblocks_fk FOREIGN KEY(number)
    REFERENCES miniblocks(number)
    ON DELETE CASCADE
);
