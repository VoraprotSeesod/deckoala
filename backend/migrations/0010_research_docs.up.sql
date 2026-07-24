-- Research documents uploaded by a user as AI source material (BRIEF-0014).
-- Owner-scoped (per USER, not per deck) so one upload can source many decks.
-- Only the EXTRACTED TEXT is kept, at <DATA_DIR>/research/<user_id>/<id>.txt —
-- the model only ever needs text, and the original binary is not re-served.
CREATE TABLE research_docs (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id),
    original_name TEXT NOT NULL,
    mime          TEXT NOT NULL,
    char_count    INTEGER NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_research_user ON research_docs (user_id, created_at);

-- Raster figures pulled out of a research PDF, so a paper's own charts can
-- illustrate the slides. Files live at
-- <DATA_DIR>/research/<user_id>/<doc_id>/<filename>; `user_id` is denormalized
-- so every lookup can be owner-scoped without a join.
CREATE TABLE research_figures (
    id         TEXT PRIMARY KEY,
    doc_id     TEXT NOT NULL REFERENCES research_docs(id),
    user_id    TEXT NOT NULL REFERENCES users(id),
    filename   TEXT NOT NULL,
    mime       TEXT NOT NULL,
    width      INTEGER NOT NULL,
    height     INTEGER NOT NULL,
    page       INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_research_figures_doc ON research_figures (doc_id);
CREATE INDEX idx_research_figures_user ON research_figures (user_id);
