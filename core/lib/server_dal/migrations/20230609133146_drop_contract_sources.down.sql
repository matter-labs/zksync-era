CREATE TABLE IF NOT EXISTS contract_sources
  (
    address BYTEA PRIMARY KEY,
    assembly_code TEXT NOT NULL,
    pc_line_mapping JSONB NOT NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
  );
