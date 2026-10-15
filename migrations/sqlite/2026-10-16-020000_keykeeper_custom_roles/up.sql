-- Keykeeper: a custom role's permissions, for a regular membership. See
-- src/db/models/keykeeper_roles.rs.
CREATE TABLE membership_permissions (
    membership_uuid TEXT NOT NULL PRIMARY KEY REFERENCES users_organizations (uuid) ON DELETE CASCADE,
    permissions     TEXT NOT NULL
);
