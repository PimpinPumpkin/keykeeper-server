-- Keykeeper: the MSP audit log. Names and emails are copied in when the event
-- happens, so the log stays readable after an account or organization is
-- deleted; only the MSP itself cascades.
CREATE TABLE msp_events (
    uuid         TEXT NOT NULL PRIMARY KEY,
    msp_uuid     TEXT NOT NULL REFERENCES msps (uuid) ON DELETE CASCADE,
    created_at   TIMESTAMP NOT NULL,
    event        TEXT NOT NULL,
    actor_uuid   TEXT,
    actor_email  TEXT,
    org_uuid     TEXT,
    org_name     TEXT,
    target_email TEXT,
    detail       TEXT,
    ip           TEXT
);
CREATE INDEX msp_events_msp_created ON msp_events (msp_uuid, created_at);
