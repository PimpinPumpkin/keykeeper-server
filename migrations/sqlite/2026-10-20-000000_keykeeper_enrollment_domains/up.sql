-- Keykeeper: the email domains whose accounts may join a client with its RMM
-- install command, as a space-separated list. A client without a row lets
-- anyone with the command join. People invited by email aren't limited. See
-- DESIGN.md, "Onboarding through the RMM".
CREATE TABLE enrollment_domains (
    org_uuid TEXT NOT NULL PRIMARY KEY REFERENCES organizations (uuid) ON DELETE CASCADE,
    domains  TEXT NOT NULL
);
