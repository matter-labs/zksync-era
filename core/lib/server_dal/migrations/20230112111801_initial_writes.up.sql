CREATE TABLE initial_writes (
      hashed_key BYTEA NOT NULL PRIMARY KEY,
      l1_batch_number BIGINT NOT NULL REFERENCES l1_batches (number) ON DELETE CASCADE,

      created_at TIMESTAMP NOT NULL,
      updated_at TIMESTAMP NOT NULL
);

CREATE INDEX initial_writes_l1_batch_number_index ON initial_writes (l1_batch_number);
