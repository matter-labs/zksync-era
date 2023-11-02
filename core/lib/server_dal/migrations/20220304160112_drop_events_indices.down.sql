CREATE INDEX events_address_idx ON events USING hash (address);
CREATE INDEX events_topic1_idx ON events USING hash (topic1);
CREATE INDEX events_topic2_idx ON events USING hash (topic2);
CREATE INDEX events_topic3_idx ON events USING hash (topic3);
CREATE INDEX events_topic4_idx ON events USING hash (topic4);
