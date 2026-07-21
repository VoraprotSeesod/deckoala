-- Baseline migration: proves the migration infrastructure works.
-- Product tables (users, decks, ...) arrive in later briefs; see docs/design/ARCHITECTURE.md §5.
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO meta (key, value)
VALUES ('schema_seeded', '1')
ON CONFLICT(key) DO NOTHING;
