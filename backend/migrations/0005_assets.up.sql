-- Assets: uploaded images referenced by decks (BRIEF-0004).
-- Files live at <DATA_DIR>/assets/<deck_id>/<filename>; served at /assets/...
-- See docs/design/ARCHITECTURE.md §5.
CREATE TABLE assets (
    id            TEXT PRIMARY KEY,
    deck_id       TEXT NOT NULL REFERENCES decks(id),
    filename      TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime          TEXT NOT NULL,
    size_bytes    INTEGER NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_assets_deck ON assets (deck_id, created_at);
CREATE UNIQUE INDEX idx_assets_deck_filename ON assets (deck_id, filename);
