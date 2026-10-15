DROP TABLE msp_enrollment_joins;
DROP INDEX msp_clients_enrollment_token;
ALTER TABLE msp_clients DROP COLUMN everyone_collection;
ALTER TABLE msp_clients DROP COLUMN enrollment_token_created_at;
ALTER TABLE msp_clients DROP COLUMN enrollment_token;
