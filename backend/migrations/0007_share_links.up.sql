-- Share links: an owner mints a token granting view/edit access to ONE deck,
-- usable without an account (BRIEF-0008). A token authorizes exactly its
-- deck_id; revocation and optional expiry are re-checked on every use.
CREATE TABLE share_links (
    id TEXT PRIMARY KEY,
    deck_id TEXT NOT NULL REFERENCES decks(id),
    token TEXT NOT NULL UNIQUE,
    permission TEXT NOT NULL CHECK (permission IN ('view', 'edit')),
    created_at TEXT NOT NULL,
    -- Canonical UTC RFC3339 (server-normalized at mint) so the active-check
    -- can compare as TEXT; NULL means "never expires".
    expires_at TEXT,
    revoked_at TEXT
);

CREATE INDEX idx_share_links_deck ON share_links (deck_id);
