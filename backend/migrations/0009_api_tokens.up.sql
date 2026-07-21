-- Per-user API tokens for the MCP endpoint (BRIEF-0011). Only a SHA-256 hash
-- of the token is stored — these are 256-bit random secrets, so a fast hash is
-- correct (argon2 is for low-entropy passwords) and lookup is one indexed match.
-- The plaintext is shown exactly once, at creation, and never again.
CREATE TABLE api_tokens (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id),
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at   TEXT
);

CREATE INDEX idx_api_tokens_user ON api_tokens (user_id);
