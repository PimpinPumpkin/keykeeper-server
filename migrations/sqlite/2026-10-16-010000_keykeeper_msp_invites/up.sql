-- Keykeeper: invitations to an MSP's staff for people who may not have an
-- account yet. Accepting needs the secret token and an account with the
-- invited email address.
CREATE TABLE msp_invites (
    uuid             TEXT NOT NULL PRIMARY KEY,
    msp_uuid         TEXT NOT NULL REFERENCES msps (uuid) ON DELETE CASCADE,
    email            TEXT NOT NULL,
    role             INTEGER NOT NULL,
    token            TEXT NOT NULL,
    invited_by_email TEXT NOT NULL,
    created_at       DATETIME NOT NULL
);
CREATE UNIQUE INDEX msp_invites_token ON msp_invites (token);
CREATE UNIQUE INDEX msp_invites_msp_email ON msp_invites (msp_uuid, email);
