-- Keykeeper: how people join an organization: whether a member who accepts an
-- invitation is confirmed automatically (by an owner's or admin's unlocked
-- app, since the server never holds the organization's key), and how long an
-- invitation lasts. An organization without its own row follows its MSP's
-- default, or outside an MSP the server's: confirmed automatically, and
-- invitations last INVITATION_EXPIRATION_HOURS. See DESIGN.md, "Joining".
CREATE TABLE org_joining (
    org_uuid     TEXT    NOT NULL PRIMARY KEY REFERENCES organizations (uuid) ON DELETE CASCADE,
    auto_confirm BOOLEAN NOT NULL,
    invite_hours INTEGER NOT NULL
);

CREATE TABLE msp_joining (
    msp_uuid     TEXT    NOT NULL PRIMARY KEY REFERENCES msps (uuid) ON DELETE CASCADE,
    auto_confirm BOOLEAN NOT NULL,
    invite_hours INTEGER NOT NULL
);
