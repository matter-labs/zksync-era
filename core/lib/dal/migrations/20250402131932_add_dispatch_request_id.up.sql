ALTER TABLE data_availability ADD COLUMN dispatch_request_id TEXT NOT NULL DEFAULT '';
ALTER TABLE data_availability ALTER COLUMN blob_id DROP NOT NULL;
