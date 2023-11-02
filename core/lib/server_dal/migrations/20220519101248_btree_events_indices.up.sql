CREATE INDEX IF NOT EXISTS events_address_idx ON events USING btree (address);
CREATE INDEX IF NOT EXISTS events_topic1_idx ON events USING btree (topic1);
CREATE INDEX IF NOT EXISTS events_topic2_idx ON events USING btree (topic2);
CREATE INDEX IF NOT EXISTS events_topic3_idx ON events USING btree (topic3);
CREATE INDEX IF NOT EXISTS events_topic4_idx ON events USING btree (topic4);
