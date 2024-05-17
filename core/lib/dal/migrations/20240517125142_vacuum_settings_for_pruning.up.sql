alter table events			set (autovacuum_vacuum_scale_factor=0,autovacuum_vacuum_threshold=50000);
alter table transactions	set (autovacuum_vacuum_scale_factor=0,autovacuum_vacuum_threshold=50000);
alter table storage_logs	set (autovacuum_vacuum_scale_factor=0,autovacuum_vacuum_threshold=50000);
alter table call_traces		set (autovacuum_vacuum_scale_factor=0,autovacuum_vacuum_threshold=50000);
alter table l1_batches		set (autovacuum_vacuum_scale_factor=0,autovacuum_vacuum_threshold=10000);
alter table miniblocks	    set (autovacuum_vacuum_scale_factor=0,autovacuum_vacuum_threshold=10000);
