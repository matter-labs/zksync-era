CREATE INDEX IF NOT EXISTS events_gin_index on events using gin ((array [
    address,
    '\x01'::bytea || topic1,
    '\x02'::bytea || topic2,
    '\x03'::bytea || topic3,
    '\x04'::bytea || topic4
]));
