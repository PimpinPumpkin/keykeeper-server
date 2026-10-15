-- Keykeeper: what each MSP technician can reach. Every client, or the ones
-- listed in msp_staff_clients; admin or a custom role inside them
-- (client_access, JSON; empty means admin); and whether they see install
-- commands. MSP admins always reach every client as owners. An invitation
-- carries the access its technician gets on joining (msp_invites.access).
ALTER TABLE msp_staff ADD COLUMN all_clients BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE msp_staff ADD COLUMN client_access TEXT;
ALTER TABLE msp_staff ADD COLUMN install_commands BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE msp_invites ADD COLUMN access TEXT;

CREATE TABLE msp_staff_clients (
    msp_uuid  TEXT NOT NULL REFERENCES msps (uuid) ON DELETE CASCADE,
    user_uuid TEXT NOT NULL REFERENCES users (uuid) ON DELETE CASCADE,
    org_uuid  TEXT NOT NULL REFERENCES organizations (uuid) ON DELETE CASCADE,
    PRIMARY KEY (msp_uuid, user_uuid, org_uuid)
);
