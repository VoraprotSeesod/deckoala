# Brief: Deck CRUD + dashboard

- **ID:** BRIEF-0002
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all), `docs/design/ARCHITECTURE.md` §5 (ERD `decks`), §6.2 (file ops), `decisions/ADR-0001-stack.md` (reserved prefixes, /data), `decisions/ADR-0002-slide-engine.md` (durable contract: deck = standard Marp Markdown), `docs/briefs/BRIEF-0001-auth-users.md` (auth patterns to reuse)
- **Depends on:** BRIEF-0001 (done — commit `c4e3072`)
- **Language:** code/identifiers/UI copy in English

## Goal
Users can create, list, open, rename, duplicate, soft-delete, import and export their own Markdown decks from a real dashboard at `/app`. The editor (BRIEF-0003) will build on the deck records and update endpoint this brief creates.

## Scope
### In scope
- **Migration `0003_decks`** (reversible pair) per ERD §5: `id TEXT PK` (uuid v4), `owner_id TEXT NOT NULL REFERENCES users(id)`, `title TEXT NOT NULL`, `markdown TEXT NOT NULL`, `theme TEXT NOT NULL DEFAULT 'deckoala'`, `created_at`/`updated_at TEXT NOT NULL` (UTC RFC3339), `deleted_at TEXT NULL` (soft delete). Index on `(owner_id, deleted_at, updated_at)`. Also enable `PRAGMA foreign_keys` via `SqliteConnectOptions::foreign_keys(true)` in `init_db` (currently missing — FKs are silently unenforced).
- **Auth extractor:** `AuthUser` (FromRequestParts) reading the session's `user_id`; absent → 401 JSON. All deck routes require it. The origin-check + session layers from BRIEF-0001 cover the new routes automatically (they nest under `/api`).
- **Endpoints** (camelCase JSON; deck object = `{id, title, theme, createdAt, updatedAt}` + `markdown` only where noted):
  - `GET /api/decks` → 200 list (NO markdown field — keep it lean), ordered `updated_at` DESC, excluding soft-deleted.
  - `POST /api/decks` `{title?, markdown?}` → 201 full deck (with markdown). Missing/empty title → `"Untitled deck"`. Missing markdown → default Marp template (below). Providing markdown = the **import path**: the SPA reads a `.md` file client-side (`File.text()`) and POSTs it — no separate upload endpoint.
  - `GET /api/decks/{id}` → 200 full deck (with markdown).
  - `PATCH /api/decks/{id}` `{title?, markdown?}` (at least one) → 200 full deck; bumps `updated_at`.
  - `DELETE /api/decks/{id}` → 204, sets `deleted_at` (soft delete — row preserved).
  - `POST /api/decks/{id}/duplicate` → 201 full deck: new id, title = source title **truncated so that title + `" (copy)"` ≤ 200 chars** (the 1–200 invariant must survive repeated duplication), same markdown/theme, fresh timestamps.
  - `GET /api/decks/{id}/export` → 200 `text/markdown; charset=utf-8`, `Content-Disposition: attachment`, body = raw markdown. Exact filename rule (post-review): plain `filename` = title with every char outside `[A-Za-z0-9 ._-]` **removed** (not replaced), trimmed; empty result → `deck`; always `.md` appended. `filename*` = RFC 5987 `UTF-8''` percent-encoding of the raw title + `.md` (Thai/emoji titles survive there). Both parts are built only from filtered/encoded bytes — no header-injection path.
  - **Body limit:** the `/api` router gets `DefaultBodyLimit::max(4 MB)` — JSON escaping can double a 1 MB markdown payload, which would trip axum's default 2 MB limit before our 1 MB cap could answer 422. The 1 MB markdown cap stays authoritative.
- **Default new-deck template** (traces ADR-0002 — standard Marp Markdown): frontmatter `marp: true`, `theme: deckoala`, `paginate: true`, then a title slide using the deck title, a `---` break, and one example slide including a KaTeX block. (`deckoala` theme CSS ships in BRIEF-0003; Marp falls back to its default theme until then — acceptable.)
- **Frontend:**
  - `/app` dashboard replaces the placeholder: "New deck" + "Import .md" buttons (import = hidden file input, `accept=".md,.markdown,text/markdown"`; precheck `file.size ≤ 1 MB` with a clear message; read text; POST with `title` = the file's basename without extension, trimmed and truncated to 200 chars — empty → omit so the server defaults); responsive card grid (single column ≤ ~600px) of decks: title, "Updated <local datetime>", actions **Open / Rename / Duplicate / Export / Delete** (rename via `prompt()`, delete via `confirm()` — dialog polish deferred `[STD]`); empty state with a friendly message + New deck CTA.
  - `/app/deck/[id]` minimal deck page (the editor replaces it in BRIEF-0003): loads the deck (404 → error page), shows title, a read-only `<pre>` of the markdown, Export and Back-to-dashboard actions, note "The editor arrives in the next build phase."
  - `lib/api.ts` gains typed `decks` methods incl. `exportUrl(id)`.
- **Backend tests** (reuse the BRIEF-0001 pattern; add an authed-request helper that registers a user and returns its cookie):
  1. every deck endpoint without a session → 401.
  2. create with defaults → 201, title `"Untitled deck"`, markdown contains `marp: true`; list shows it WITHOUT a markdown field.
  3. get own deck → full markdown present.
  4. **owner scoping:** user B hitting user A's deck (GET/PATCH/DELETE/duplicate/export) → **404** (not 403 — no existence leak) `[STD]`.
  5. rename → title changes, `updated_at` bumps (sleep a few ms first), empty title → 422.
  6. markdown > 1 MB on create or patch → 422 (cap `[STD]`; axum's 2 MB body limit stays).
  7. duplicate → new id, `" (copy)"` title, identical markdown.
  8. soft delete → 204; GET → 404; list empty; **row still in DB with `deleted_at` set** (direct SQL assert); **and the tombstone is dead on every route**: PATCH / duplicate / export / a second DELETE against the deleted deck → 404 as the owner (an implementation that forgets `deleted_at IS NULL` on the write paths must fail here).
  9. export → 200, `text/markdown`, `Content-Disposition` attachment, body equals markdown; all-Thai title → exact ASCII fallback `filename="deck.md"` + decodable `filename*`; a title containing `"` and emoji yields a disposition with no quote-escape/CR/LF issues.
  10. create with provided markdown (import path) → stored verbatim.
  11. migration roundtrip: undo to version 2 → `decks` table gone; re-run → back.
  12. duplicate of a deck with a 200-char title → 201, result title ≤ 200 chars and ends with `" (copy)"`.
  13. title containing control characters (e.g. `\n` or `\u{7}`) on create or PATCH → 422.

### Out of scope (later)
- Editor + live preview + autosave/revisions (BRIEF-0003); assets (0004); sharing (0008)
- Trash/restore UI or endpoint (soft-deleted rows are simply preserved for now), pagination/search (personal scale), deck settings/theme picker

## Data model
`decks` exactly per ERD §5 as specified above. No other tables touched.

## Business rules (the important bits)
- **Owner-scoping invariant (CLAUDE.md §2):** every deck query — reads AND writes — filters `owner_id = ?` AND `deleted_at IS NULL`; cross-owner access is indistinguishable from nonexistent (404).
- Title: trimmed, 1–200 chars after trim, **no control characters (C0 incl. CR/LF/TAB) → 422** (create defaults empty → `"Untitled deck"`; PATCH with empty/whitespace title → 422). Titles are NOT unique.
- `now_rfc3339` must emit **fixed-width** timestamps (constant subsecond digits) so the TEXT `updated_at DESC` ordering is lexicographically monotonic — `time`'s well-known RFC3339 trims trailing zeros and would misorder same-second rows.
- Markdown: valid UTF-8 (JSON guarantees it), ≤ 1,000,000 bytes on create/patch/import → else 422.
- `updated_at` bumps on PATCH only (rename or markdown change); duplicate gets fresh `created_at = updated_at = now`.
- Timestamps UTC RFC3339 (project invariant). Deck format stays standard Marp Markdown (ADR-0002 durable contract — no proprietary fields).

## API / interface surface
See Scope. `/api/auth/*`, `/api/instance`, `/api/health` unchanged.

## Deliverables
- Migration pair `0003_decks`, `foreign_keys(true)` in init_db, `src/decks.rs` handlers + `AuthUser` extractor, dashboard + deck page + api.ts, tests 1–11 executed.

## Verification / acceptance gate
- **Runs proof:** `docker compose -p deckoala up -d --build` succeeds; health unchanged (`status="ok"`, `db="ok"`, `chromium=true`); **UI flow in a real browser:** login → create deck → appears in dashboard → rename → duplicate → open deck page (markdown visible) → export downloads a `.md` → delete removes it from the dashboard — desktop **and** ~375px mobile viewport (no horizontal scroll, actions reachable).
- curl proof: authed create 201 → list contains it → patch rename 200 → duplicate 201 → export has `Content-Disposition` → delete 204 → get 404; unauthed list → 401.
- Tests executed: `cargo test` exit 0, ≥ 32 total passing (19 existing + ≥ 13 new); `npm run check` exit 0; `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` clean (Docker verify stage per precedent).
- Browser flow additionally exercises **Import .md** (title comes from the filename).
- Isolation unchanged (only port 8321, `deckoala-*` resources).
- Run the `pr-review` skill before committing.
