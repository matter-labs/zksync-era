ALTER TABLE transactions ADD COLUMN force_fail BOOLEAN;

CREATE TABLE force_failed_priority_ops (
    priority_op_id BIGINT PRIMARY KEY,
    reason         TEXT,
    created_by     TEXT NOT NULL,
    created_at     TIMESTAMP NOT NULL DEFAULT now()
);
