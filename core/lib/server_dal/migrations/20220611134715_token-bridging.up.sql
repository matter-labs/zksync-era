ALTER TABLE tokens ADD COLUMN l1_address BYTEA NOT NULL;
ALTER TABLE tokens ADD COLUMN l2_address BYTEA NOT NULL;
ALTER TABLE tokens DROP COLUMN address;
ALTER TABLE tokens ADD PRIMARY KEY (l1_address);
CREATE UNIQUE INDEX l2_address_index on tokens (l2_address);
