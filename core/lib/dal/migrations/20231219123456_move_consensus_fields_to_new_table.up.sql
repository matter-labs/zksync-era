ALTER TABLE miniblocks DROP COLUMN consensus;

CREATE TABLE miniblocks_consensus (
  number BIGINT NOT NULL,
  fields JSONB NOT NULL,
  PRIMARY KEY(number),
  CONSTRAINT miniblocks_fk FOREIGN KEY(number)
    REFERENCES miniblocks(number)
    ON DELETE CASCADE
);
