-- Keykeeper: joining a client organization through the RMM install command.
-- Each client has an enrollment token, and its "Everyone" collection, which
-- people who join get. Joins are recorded.
ALTER TABLE msp_clients ADD COLUMN enrollment_token TEXT;
ALTER TABLE msp_clients ADD COLUMN enrollment_token_created_at DATETIME;
ALTER TABLE msp_clients ADD COLUMN everyone_collection TEXT;
CREATE UNIQUE INDEX msp_clients_enrollment_token ON msp_clients (enrollment_token);

CREATE TABLE msp_enrollment_joins (
    membership_uuid TEXT NOT NULL PRIMARY KEY REFERENCES users_organizations (uuid) ON DELETE CASCADE,
    org_uuid        TEXT NOT NULL REFERENCES organizations (uuid) ON DELETE CASCADE,
    joined_at       DATETIME NOT NULL
);
CREATE INDEX msp_enrollment_joins_org ON msp_enrollment_joins (org_uuid);
