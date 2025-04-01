CREATE TABLE consensus_committees (
  active_at_block BIGINT PRIMARY KEY,
  validators JSONB NOT NULL,
);