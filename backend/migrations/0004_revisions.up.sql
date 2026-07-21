-- Revisions: restorable snapshots of deck markdown (BRIEF-0003).
-- See docs/design/ARCHITECTURE.md §5 + BRIEF-0003 snapshot policy.
CREATE TABLE revisions (
    id         TEXT PRIMARY KEY,
    deck_id    TEXT NOT NULL REFERENCES decks(id),
    markdown   TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_revisions_deck ON revisions (deck_id, created_at);
