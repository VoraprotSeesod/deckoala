# Brief: Sharing — share links (view / edit)

- **ID:** BRIEF-0008
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all, esp. §2 owner-scoped multi-user + share-link tokens, §3 runtime isolation), `docs/design/ARCHITECTURE.md` §4 (ERD `share_links`), §6.6 (sharing flow), line "all queries filter by owner **or** a valid share token", `decisions/ADR-0001-stack.md`, `backend/src/decks.rs` (owner-scoped CRUD + the revision/snapshot transaction — the single source of truth to REUSE, not duplicate), `backend/src/export.rs` (print-token + print-cookie pattern + PDF render core to reuse), `backend/src/assets.rs` (`authorize_serve` to extend), `frontend/src/routes/app/deck/[id]/+page.svelte` + `frontend/src/routes/present/[id]/+page.svelte` (editor + present surfaces to reuse for the shared views), `frontend/src/lib/api.ts`
- **Depends on:** BRIEF-0002 (deck CRUD), BRIEF-0003 (revisions), BRIEF-0006 (PDF export) — all done
- **Language:** code/identifiers/UI copy in English

## Goal
The user's "สามารถ share ให้คนอื่นได้": a deck owner mints **share links** for one of their decks — permission **view** or **edit** — and hands the link to anyone. The recipient opens `/s/{token}` and needs **no account**. A **view** link renders the deck read-only and lets the recipient present it fullscreen and download a PDF; an **edit** link opens the editor bound to the token (edit markdown & title, upload images, browse/restore revisions, present, export PDF). Links are **revocable** and may carry an **optional expiry**. The no-external-CDN and loopback-only invariants are unchanged; a share link only ever grants access to the ONE deck it was minted for.

## Scope
### Backend
- **Migration 0007 `share_links`** (additive; reversible down drops the table).
- **`shares.rs` module** — two families of handlers:
  - **Owner-side** (session `AuthUser`, deck owner-checked → 404 on non-owner, mirroring decks):
    - `POST /api/decks/{id}/shares` — body `{ permission: "view"|"edit", expiresAt?: RFC3339 }` → mint a high-entropy token, insert, return the link DTO (incl. the token/URL so the owner can copy it). 201.
    - `GET /api/decks/{id}/shares` — list this deck's links with computed `status` (`active`|`revoked`|`expired`).
    - `DELETE /api/decks/{id}/shares/{shareId}` — set `revoked_at` (idempotent-ish: already-revoked still 204; unknown link 404). 204.
  - **Token-side** (NO session; the token in the path is the authorization; resolve to an *active* `share_link` → `deck_id` + `permission`; a missing/revoked/expired token is a uniform **404**):
    - `GET /api/s/{token}` — returns `{ deckId, title, markdown, theme, permission }` for view+edit; **sets the share cookie** (see below). 200.
    - `PATCH /api/s/{token}` — **edit only** (403 on a view token); update `markdown`/`title` with the SAME revision-snapshot transaction as the owner path, by resolving the deck's real `owner_id` and delegating to the existing `decks.rs` helpers. Returns the updated deck. Validation identical to the owner PATCH (title length/control-char, 1 MB markdown).
    - `GET /api/s/{token}/revisions`, `GET /api/s/{token}/revisions/{revId}`, `POST /api/s/{token}/revisions/{revId}/restore` — **edit only**.
    - `POST /api/s/{token}/assets` — **edit only** image upload; reuse the asset upload core (magic-byte sniff, 5 MB, 8 MB route limit) with the resolved `deck_id`.
    - `POST /api/s/{token}/export/pdf` — view+edit; reuse the export render/cache core keyed by `deck_id`.
- **Extend `assets.rs` `authorize_serve`**: session-owner OR valid print cookie OR a **valid share cookie for this deck** (so an anonymous `/s/{token}` viewer's `<img src="/assets/{deck}/…">` loads). The share cookie carries the raw share token (HttpOnly, SameSite=Lax, Path=/); it is re-resolved to an active `share_link` on every asset request, so **revocation/expiry take effect immediately**.
- **Reuse, don't fork:** the revision/snapshot logic (`update_in_tx`, `restore_in_tx`, `insert_revision`, `finish_deck_tx`, `fetch_deck`, `fetch_revision`, title parsing) and the PDF render core (`render_pdf` + cache) stay single-source; expose the minimum as `pub(crate)` and have the token handlers resolve `owner_id` from the deck row, then call them unchanged.

### Frontend
- **Share management UI** on the editor page (and a Share entry on dashboard cards): a "Share" panel listing existing links with status + copy-URL button, a "New link" control (view/edit + optional expiry), and a revoke (×) per link. Owner-only.
- **`/s/[token]` route** (public, outside the `/app` guard): fetch `GET /api/s/{token}`.
  - **view** → read-only render (reuse the deck render + the present component; expose a "Present" button + "Download PDF" button).
  - **edit** → the editor bound to the token (reuse the editor: autosave via `PATCH /api/s/{token}`, image paste/drop via the token asset endpoint, revisions panel via the token revision endpoints, Present + Download PDF).
  - A small banner makes the mode obvious ("You're editing a shared deck" / "Shared view").
- **API client** (`api.ts`): `api.shares` (list/create/revoke for owners) and `api.shared` (byToken load/update/revisions/asset-upload/export for the recipient).

### Out of scope (later, note in SESSION_STATE)
- Per-user collaborator invites / real-time multi-cursor (this is anonymous token sharing, last-write-wins as today).
- Public deck discovery / listings, comments, view analytics.
- Rotating a link's token in place (revoke + mint a new one instead).
- Email delivery of links (the owner copies the URL).

## Data model
`share_links { id TEXT PK, deck_id TEXT NOT NULL (FK decks.id), token TEXT NOT NULL UNIQUE, permission TEXT NOT NULL CHECK(permission IN ('view','edit')), created_at TEXT NOT NULL, expires_at TEXT NULL, revoked_at TEXT NULL }` — index on `deck_id`; `token` already unique-indexed. Token = 32 random bytes, URL-safe base64 (no padding). Timestamps UTC RFC3339.

## Business rules (the important bits)
- **A token authorizes exactly ONE deck** (its `deck_id`) regardless of who opens it; it never widens to the owner's other decks. Every token-side query is `deck_id`-scoped.
- **Active** = `revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now)`. Inactive/unknown token → uniform 404 (no existence oracle), same as foreign-deck 404.
- **Edit ≠ ownership.** An edit token can change content (markdown/title), upload images, and restore revisions of its deck — but can NOT delete/duplicate the deck, mint/list/revoke share links, or touch instance fonts. Those stay session-owner/admin only.
- **Deleted deck:** if the deck was soft-deleted, all its tokens resolve 404 (the `deleted_at IS NULL` filter is kept on every token query).
- **Owner-only management:** minting/listing/revoking links requires the session owner of the deck (404 otherwise). The list may return token values (the owner already holds them) but never over a share token.
- **Token entropy + constant work:** 256-bit token; lookups are by exact token match (indexed). Do not log tokens.
- **Cookie scope:** the share cookie only *adds* asset-read authorization for the deck its token resolves to; it grants nothing else and is ignored by every owner/admin route.

## API / interface surface
Owner: `POST/GET /api/decks/{id}/shares`, `DELETE /api/decks/{id}/shares/{shareId}`.
Token: `GET /api/s/{token}`, `PATCH /api/s/{token}`, `GET /api/s/{token}/revisions[/{revId}]`, `POST /api/s/{token}/revisions/{revId}/restore`, `POST /api/s/{token}/assets`, `POST /api/s/{token}/export/pdf`. SPA: `/s/{token}` (view/edit) reachable without the `/app` auth guard.

## Deliverables
Migration 0007 (up/down) · `shares.rs` (+ router wiring, `authorize_serve` extension, minimal `pub(crate)` exposure in decks/export) · backend tests (owner mint/list/revoke + owner-scoping 404; token view vs edit permission split; PATCH via edit token snapshots a revision; expired/revoked → 404; asset serve via share cookie; token authorizes only its own deck) · frontend `/s/[token]` view+edit + share panel + `api.ts` methods.

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): `docker compose -p deckoala up -d --build` healthy + `/api/health`; `cargo test` (visible pass count, exit 0) + `cargo fmt --check` + `cargo clippy -D warnings` + `npm run check` + vitest; `pr-review` PASS before commit.
- Runtime, in the browser: as owner, mint a **view** link and an **edit** link for a deck. In a **fresh/incognito context (no session)**: open the view link → slides render (with images), Present works, Download PDF returns a `%PDF-`; confirm PATCH is refused (view). Open the edit link → edit markdown, it autosaves (a revision is snapshotted), paste an image and see it render, Present + PDF work. **Revoke** the edit link → the same URL now 404s and can no longer read or write. Confirm a token for deck A cannot read deck B (swap ids → 404). Network tab shows **zero external requests** (invariant preserved).
- Adversarial review (2 lenses — implementability + security/authz) before AND after coding, findings folded, as with prior briefs.

## Folded review findings (pre-code adversarial review, 2026-07-21 — 30 agents)
Confirmed (fix in this brief):
1. **Expiry must be canonicalized, not string-compared.** `now_rfc3339()` is fixed-width UTC so TEXT sorts chronologically; a raw client `expiresAt` like `…T18:00:00+07:00` (real 11:00Z) byte-compares GREATER than a `…Z` now → the link stays active ~7h past expiry (fail-open). At mint, **parse `expiresAt` → convert to UTC → re-serialize via `now_rfc3339()`** before storing; reject unparseable/naive/no-offset input with **422**. Active-check stays a TEXT comparison, now sound. Regression test: a `+07:00` expiry that must **404 after the real instant** passes.
2. **Present must be token-aware.** `present/[id]` loads via owner-scoped `api.decks.get` and its Exit/presenter routes point at `/app/deck/{id}` (→ `/login` for an anonymous viewer). The shared view's Present must load the deck from the token, Exit back to `/s/{token}`, and open a **token-scoped** presenter — implemented as a share-context on the present surface (or a self-contained present view inside `/s/[token]`), never a bare `/present/{id}` navigation.
3. **Add `GET /api/s/{token}/export`** (Markdown, view+edit) mirroring `decks::export` by `deck_id`, so the reused editor's "Export .md" control is live in share mode (rather than shipping a dead button).
4. **Anonymous-token exports get their own concurrency bound.** An edit token can PATCH a 1-byte change to force a cache miss and a fresh Chromium render; sharing the owner `export_sem` lets a leaked edit link starve owner exports. Add a **separate `share_export_sem` (permits = 1)** that the token export path acquires (the owner path keeps its own pool), and note that an edit link grants bounded render capacity to whoever holds the URL.

Folded for correctness/robustness (from verified-but-non-blocking notes):
5. **Token `GET /api/s/{token}` returns the full deck shape** `{ id, title, theme, markdown, createdAt, updatedAt, permission }` (NOT a trimmed `{deckId,…}`) — the reused editor seeds its optimistic-concurrency baseline from `updatedAt` and keys deck-switch effects on `id`; token `PATCH` accepts `baseUpdatedAt` and returns `updatedAt`, so token edits get the same clobber protection.
6. **`pub(crate)` cores return `Response`/public types, never the private `DeckRow`/`RevisionRow`** — exposing a fn that returns a module-private struct trips `private_interfaces`, which `-D warnings` turns into a build failure. Expose `pub(crate)` orchestrators (e.g. `get_deck_core`/`update_deck_core`/`revisions_*_core`/`render_deck_pdf`/`store_asset`) that take `deck_id`+`owner_id` and return `Response`; keep the row structs and `update_in_tx`/`restore_in_tx` private. Owner handlers become thin wrappers over the same cores.
7. **Per-deck share cookie.** One fixed-name `Path=/` cookie can't represent two shared decks (opening B overwrites A's token → A's images break). Name it **`deckoala_share_{deckId}`** and scope it `Path=/assets/{deckId}`; re-resolve its token to an active share on every asset request (revocation/expiry immediate). Set **`Secure` from `state.secure_cookie`**, `HttpOnly`, `SameSite=Lax`.
8. **Double-scope the revoke/list queries.** `DELETE /api/decks/{id}/shares/{shareId}` must be `WHERE id=? AND deck_id=?` AFTER confirming the caller owns `{id}` — a `shareId`-only write would let any owner revoke another owner's link. List is `WHERE deck_id=?` for an owned deck only.
9. **`Referrer-Policy: no-referrer`** response header (the token is a bearer secret in the URL path). Cheap defense-in-depth alongside the existing CSP.

Not-a-problem (verified): the CSRF `same_origin_guard` does NOT block same-origin token mutations (browser sends `Origin==Host`), and token-edit reuse of the revision transaction is sound (`owner_id` is purely a WHERE filter) — place the token routes under `/api` as normal; do not move the middleware.
