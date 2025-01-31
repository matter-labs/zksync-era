ALTER TABLE vm_runner_protective_reads ALTER COLUMN processing_started_at TYPE TIME USING (null);
ALTER TABLE vm_runner_bwip ALTER COLUMN processing_started_at TYPE TIME USING (null);
