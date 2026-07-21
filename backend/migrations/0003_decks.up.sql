-- Decks: core content records (BRIEF-0002). See docs/design/ARCHITECTURE.md §5.
CREATE TABLE decks (
    id         TEXT PRIMARY KEY,
    owner_id   TEXT NOT NULL REFERENCES users(id),
    title      TEXT NOT NULL,
    markdown   TEXT NOT NULL,
    theme      TEXT NOT NULL DEFAULT 'deckoala',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX idx_decks_owner ON decks (owner_id, deleted_at, updated_at);
