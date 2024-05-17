alter table events			reset (autovacuum_vacuum_scale_factor,autovacuum_vacuum_threshold);
alter table transactions	reset (autovacuum_vacuum_scale_factor,autovacuum_vacuum_threshold);
alter table storage_logs	reset (autovacuum_vacuum_scale_factor,autovacuum_vacuum_threshold);
alter table call_traces		reset (autovacuum_vacuum_scale_factor,autovacuum_vacuum_threshold);
alter table l1_batches		reset (autovacuum_vacuum_scale_factor,autovacuum_vacuum_threshold);
alter table miniblocks		reset (autovacuum_vacuum_scale_factor,autovacuum_vacuum_threshold);
