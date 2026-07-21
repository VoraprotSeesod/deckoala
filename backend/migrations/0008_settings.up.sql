-- Instance-level settings (BRIEF-0010): AI provider config + the
-- root-password-is-default flag. Key/value so later briefs can add keys
-- without a migration. Secrets live here and are NEVER returned by the API.
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
