-- Keykeeper: MSPs, their staff, their client organizations, and the
-- memberships an MSP created in its clients. Every reference cascades, so
-- deleting a user, organization, or membership upstream cleans these up and
-- is never blocked by them.
CREATE TABLE msps (
    uuid       TEXT NOT NULL PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL
);

CREATE TABLE msp_staff (
    msp_uuid   TEXT NOT NULL REFERENCES msps (uuid) ON DELETE CASCADE,
    user_uuid  TEXT NOT NULL REFERENCES users (uuid) ON DELETE CASCADE,
    role       INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL,
    PRIMARY KEY (msp_uuid, user_uuid)
);

CREATE TABLE msp_clients (
    org_uuid   TEXT NOT NULL PRIMARY KEY REFERENCES organizations (uuid) ON DELETE CASCADE,
    msp_uuid   TEXT NOT NULL REFERENCES msps (uuid) ON DELETE CASCADE,
    created_at TIMESTAMP NOT NULL
);

CREATE TABLE msp_memberships (
    membership_uuid TEXT NOT NULL PRIMARY KEY REFERENCES users_organizations (uuid) ON DELETE CASCADE,
    msp_uuid        TEXT NOT NULL REFERENCES msps (uuid) ON DELETE CASCADE
);

CREATE INDEX msp_staff_user ON msp_staff (user_uuid);
CREATE INDEX msp_clients_msp ON msp_clients (msp_uuid);
CREATE INDEX msp_memberships_msp ON msp_memberships (msp_uuid);
