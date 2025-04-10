ALTER TABLE data_availability DROP COLUMN IF EXISTS dispatch_request_id;
ALTER TABLE data_availability ALTER COLUMN blob_id SET NOT NULL DEFAULT '';
