-- Keykeeper: each MSP's default policies, one row per policy type; a type
-- without a row is off. A client follows a default while its own policy does
-- the same thing, so nothing records which clients follow. MSPs that already
-- exist start with the Business preset's policies, which their Business
-- clients already have. See DESIGN.md, "Default policies".
CREATE TABLE msp_policies (
    msp_uuid TEXT    NOT NULL REFERENCES msps (uuid) ON DELETE CASCADE,
    atype    INTEGER NOT NULL,
    enabled  BOOLEAN NOT NULL,
    data     TEXT    NOT NULL,
    PRIMARY KEY (msp_uuid, atype)
);

-- Account recovery with automatic enrollment, single organization, and
-- master password requirements (see business_policies in the server).
INSERT INTO msp_policies (msp_uuid, atype, enabled, data)
    SELECT uuid, 8, 1, '{"autoEnrollEnabled":true}' FROM msps;
INSERT INTO msp_policies (msp_uuid, atype, enabled, data)
    SELECT uuid, 3, 1, 'null' FROM msps;
INSERT INTO msp_policies (msp_uuid, atype, enabled, data)
    SELECT uuid, 1, 1, '{"minComplexity":3,"minLength":12,"requireUpper":false,"requireLower":false,"requireNumbers":false,"requireSpecial":false,"enforceOnLogin":false}' FROM msps;
