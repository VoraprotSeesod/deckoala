# Brief: Editor + Marp live preview + autosave + revisions

- **ID:** BRIEF-0003
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all), `docs/design/ARCHITECTURE.md` §3 (frontend), §5 (ERD `revisions`), §6.1 (edit/preview/autosave flow), `decisions/ADR-0002-slide-engine.md` (Marp Core client-side — durable contract), `docs/briefs/BRIEF-0002-deck-crud.md` (PATCH endpoint + patterns), `backend/src/decks.rs` (extend, don't fork patterns)
- **Depends on:** BRIEF-0002 (done — commit `b119fa9`)
- **Language:** code/identifiers/UI copy in English

## Goal
The deck page becomes a real editor: CodeMirror 6 on the left, live Marp-rendered slides on the right (the `deckoala` theme — brand colors, Thai-capable fonts, KaTeX math), autosaving as you type, with restorable revision snapshots. This delivers the user's core request: "เขียน Markdown แล้ว Preview ไปพร้อม ๆ กัน".

## Scope
### In scope — backend
- **Migration `0004_revisions`** (reversible pair) per ERD §5: `id TEXT PK` (uuid), `deck_id TEXT NOT NULL REFERENCES decks(id)`, `markdown TEXT NOT NULL`, `created_at TEXT NOT NULL`; index `(deck_id, created_at)`.
- **Snapshot policy** (all decisions `[STD]`, hardened post-review):
  - On `PATCH /api/decks/{id}` where `markdown` is present AND differs from the stored value: inside one **write transaction (`BEGIN IMMEDIATE`)**, snapshot the **pre-update** markdown as a revision iff (a) the deck's newest revision is **older than the snapshot interval** (or none exists), **OR (b) the request's `baseUpdatedAt` (below) mismatches the stored `updated_at`** — a stale-base write is exactly the clobber case where the safety net must fire regardless of the interval. Then apply the update. First-ever edit therefore snapshots the original template.
  - **`baseUpdatedAt` (optional PATCH field):** the client sends the `updatedAt` its edit was based on; the server still applies last-write-wins, but a mismatch with changed markdown forces the snapshot. The editor always sends it and refreshes its baseline from each PATCH/restore response.
  - Deferred transactions are forbidden here: concurrent autosaves under SQLite WAL would hit `SQLITE_BUSY_SNAPSHOT` (500s) or double-snapshot. `BEGIN IMMEDIATE` (raw statement on an acquired connection) + `busy_timeout(5 s)` on the connect options serialize the check-insert-prune-update sequence.
  - The deck `UPDATE` inside both transactions keeps the `owner_id` + `deleted_at IS NULL` predicate and **rows_affected = 0 rolls the whole transaction back → 404** (no stray snapshot on a tombstoned deck).
  - Title-only PATCH never snapshots. Identical-markdown PATCH does not snapshot (may still update title).
  - Snapshot interval: **300 s**, carried in `AppState` (`revision_min_secs: i64`; `main` hardcodes 300, tests construct smaller/zero — no new env var).
  - **Restore always snapshots** the current markdown first (explicit user action = guaranteed restore point), then sets deck markdown, bumps `updated_at`, returns the full deck. Same `BEGIN IMMEDIATE` + rollback rules.
  - Retention: keep the **newest 50** revisions per deck; prune inside the same transaction as the insert. All "newest" ordering (list endpoint, interval check, prune subquery) uses the deterministic tie-break **`ORDER BY created_at DESC, rowid DESC`** (uuid ids are random; same-microsecond ties must not delete the fresh snapshot).
- **Endpoints** (owner-scoped through the deck — foreign/soft-deleted deck → 404, same as BRIEF-0002; camelCase):
  - `GET /api/decks/{id}/revisions` → 200 `[{id, createdAt, sizeBytes}]` newest first (no markdown — lean). `sizeBytes` = **bytes**, computed as `LENGTH(CAST(markdown AS BLOB))` — SQLite's plain `LENGTH()` on TEXT counts characters and would under-report Thai content ~3×; one test must assert a Thai revision's `sizeBytes` exceeds its char count.
  - `GET /api/decks/{id}/revisions/{revId}` → 200 `{id, createdAt, markdown}`; unknown/foreign revId → 404.
  - `POST /api/decks/{id}/revisions/{revId}/restore` → 200 full deck (per policy above).

### In scope — frontend
- **Deps:** `codemirror` (v6 meta pkg), `@codemirror/lang-markdown`, `@marp-team/marp-core`, `katex` (for its CSS/fonts served locally by Vite).
- **Render module `src/lib/marp.ts`:** single shared `Marp` instance — `html: false` (raw HTML in markdown stays escaped — XSS defense; decks get shared cross-user in BRIEF-0008), `math: 'katex'`, `inlineSVG` on so slides scale to their container; registers the **`deckoala` theme** (below); exports `renderDeck(markdown): {html, css, slideCount}`.
- **`deckoala` Marp theme** (`src/lib/theme-deckoala.ts` as a CSS string starting `/* @theme deckoala */`): based on Marp's `default` theme; `section` background `#F8F8FF`, text `#0B1215`, `font-family: 'Inter', 'Noto Sans Thai', system-ui, sans-serif`; readable heading/code/pagination styling consistent with the brand.
- **Preview isolation:** rendered `{css, html}` goes into a **Shadow DOM** container so Marp CSS can never leak into the app UI (document-level `@font-face` still reaches shadow content). Preview shows all slides in a scrollable column + a slide counter.
- **ZERO external requests invariant (CLAUDE.md §2):** editing/previewing (incl. `$$…$$` math) must trigger no request leaving the instance. The actual lever (post-review): marp-core's katex CSS lands **inside the shadow root** via the `css` string `render()` returns, and its `@font-face` URLs are prefixed by marp-core's **`katexFontPath` option, which defaults to the jsDelivr CDN** — so configure `math: { lib: 'katex', katexFontPath: '/katex-fonts/' }` and ship the `katex` npm package's `dist/fonts/*.woff2` under `frontend/static/katex-fonts/` (committed). Do NOT rely on a document-level `katex.min.css` import (its class rules can't cross the shadow boundary and it can't rewrite the runtime-generated CSS). `math: 'mathjax'` remains the fallback only if this still fails.
- **Content-Security-Policy (defense for the invariant + BRIEF-0008 groundwork):** `html: false` stops script injection but NOT network egress — `![](https://attacker/px.png)` or a `backgroundImage` directive turns a shared/imported deck into a tracking pixel. The backend adds a CSP header to every response: `default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'self'` (`script-src` keeps `'unsafe-inline'` because SvelteKit's bootstrap is an inline script; the egress-relevant directives are strict). One backend test asserts the header; the browser network gate includes a deck referencing an external image and confirms the request is CSP-blocked.
- **Editor page** (replaces the read-only `/app/deck/[id]`):
  - Top bar: back link; **inline-editable title** (PATCH on blur/Enter; validation errors surfaced); save status ("Saving…" / "Saved HH:MM" / "Save failed — retrying"); Export `.md`; Revisions toggle.
  - CodeMirror 6 (markdown language, line wrapping) ← → preview pane. Debounce: render ~150 ms after last keystroke; autosave PATCH ~2 s after last change, only when dirty; failed autosave retries on next change/interval and warns via the status chip.
  - `beforeunload` guard while dirty (native confirm) `[STD]`. Concurrency: two tabs = last-write-wins — accepted for now, revisions are the safety net; noted here deliberately `[STD]`.
  - **Revisions panel:** list (localized datetime + size); selecting a revision shows it in the preview (clear "viewing old version" indicator) with "Restore this version" (confirm → **cancel the pending autosave timer and disregard any in-flight PATCH first** — otherwise a queued autosave fires seconds later with pre-restore markdown, is interval-suppressed, and silently undoes the restore → POST restore → editor + preview + `baseUpdatedAt` baseline swap to the response) and "Back to current".
  - **Responsive:** ≥ ~900px split pane; below that, Write/Preview tab toggle; 375 px flow must work with no horizontal scroll.
- `lib/api.ts`: typed `revisions` methods.

### Out of scope (later)
- Slide thumbnail rail + drag & drop reorder + asset upload (BRIEF-0004); present mode (0005); PDF (0006); font manager (0007) — the theme's font list is fixed for now; sharing (0008); realtime co-editing (not planned — last-write-wins stands)

## Data model
`revisions` as above. `decks` untouched schema-wise (PATCH behavior extended).

## Business rules (the important bits)
- Snapshot/restore/retention policy exactly as in Scope (each rule `[STD]`, traced to ARCHITECTURE §6.1's "server snapshots a revisions row at most every 5 minutes").
- Revision routes inherit BRIEF-0002's owner-scoping-as-404 invariant (checked through the parent deck, plus `revisions.deck_id` match).
- All markdown size/title rules from BRIEF-0002 still apply to PATCH (restore content comes from our own DB and is exempt from the 1 MB re-check — it already passed it).
- Rendered preview must remain XSS-safe: `html: false` is locked; no `{@html}` of user content anywhere outside the shadow container built from Marp output.

## API / interface surface
Three new revision routes above; PATCH gains snapshot behavior. Everything else unchanged.

## Deliverables
- Migration pair `0004_revisions`, revisions module + PATCH snapshot logic (transactional), `marp.ts` + theme + editor page + revisions panel + api.ts additions, tests below executed.

## Verification / acceptance gate
- **Runs proof:** `docker compose -p deckoala up -d --build` succeeds; health unchanged. **UI flow in a real browser (desktop + 375 px):** open a deck → editor + preview render the Marp template with the `deckoala` theme (ghost-white slide, ink text); type text + a new `---` slide + `$$E=mc^2$$` → preview updates live, slide count changes, math renders; wait for autosave ("Saved") → reload → content persisted; rename via the title field; open Revisions → snapshot listed → view an old revision → Restore → editor shows restored content; mobile: tab toggle works.
- **Network proof:** browser network log during editing/preview/math shows zero requests to any non-`localhost:8321` origin.
- curl proof: PATCH with new markdown → `GET …/revisions` non-empty; `GET …/revisions/{id}` returns markdown; `restore` returns the deck with that markdown; foreign user → 404 on all three.
- Tests executed: `cargo test` exit 0 with **every enumerated behavior covered** (raw count expected ~46–50; the behavior list is the gate, not the number): snapshot-on-first-edit (pre-update content), interval suppression + elapsed-interval snapshot, **stale-`baseUpdatedAt` forces snapshot despite interval**, **two concurrent PATCHes → no 500 and exactly one snapshot (interval 300)**, title-only no-snapshot, identical-markdown no-snapshot, list newest-first lean + **Thai `sizeBytes` > char count**, get returns markdown, restore snapshots + swaps content, foreign/soft-deleted 404s on all revision routes, **cross-deck revId → 404 for GET and restore (same owner)**, retention cap 50, **CSP header present on responses**, migration roundtrip to v3. `npm run check` exit 0; `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` clean (Docker verify stage).
- Isolation unchanged. Run the `pr-review` skill before committing.
