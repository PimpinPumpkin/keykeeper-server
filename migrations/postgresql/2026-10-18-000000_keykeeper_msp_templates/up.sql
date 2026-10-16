-- Keykeeper: item templates an MSP sends to its clients. A template is an
-- encrypted item the server can't read. msp_templates records which item is
-- the source and whether every client gets it (or the clients in
-- msp_template_clients); msp_template_copies records each client's copy and
-- which version of the source (its revision date) it was made from; and
-- msp_clients.templates_collection is the client's Templates collection. See
-- DESIGN.md, "MSP templates".
CREATE TABLE msp_templates (
    msp_uuid           TEXT      NOT NULL REFERENCES msps (uuid) ON DELETE CASCADE,
    template_id        TEXT      NOT NULL,
    source_cipher_uuid TEXT      NOT NULL,
    all_clients        BOOLEAN   NOT NULL,
    created_at         TIMESTAMP NOT NULL,
    PRIMARY KEY (msp_uuid, template_id)
);

CREATE TABLE msp_template_clients (
    msp_uuid    TEXT NOT NULL,
    template_id TEXT NOT NULL,
    org_uuid    TEXT NOT NULL REFERENCES organizations (uuid) ON DELETE CASCADE,
    PRIMARY KEY (msp_uuid, template_id, org_uuid),
    FOREIGN KEY (msp_uuid, template_id) REFERENCES msp_templates (msp_uuid, template_id) ON DELETE CASCADE
);

CREATE TABLE msp_template_copies (
    org_uuid        TEXT NOT NULL REFERENCES organizations (uuid) ON DELETE CASCADE,
    template_id     TEXT NOT NULL,
    cipher_uuid     TEXT NOT NULL REFERENCES ciphers (uuid) ON DELETE CASCADE,
    source_revision TEXT NOT NULL,
    PRIMARY KEY (org_uuid, template_id)
);

ALTER TABLE msp_clients ADD COLUMN templates_collection TEXT;
