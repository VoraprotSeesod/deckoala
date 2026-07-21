# Brief: Slide thumbnail rail (reorder) + image asset upload

- **ID:** BRIEF-0004
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all), `docs/design/ARCHITECTURE.md` §3 §4 (reserved `/assets/` prefix), §5 (ERD `assets`), §6.5 (fonts note for MIME parallels), `decisions/ADR-0001-stack.md` (reserved prefixes, `/data`), `docs/briefs/BRIEF-0003-editor-preview.md` (editor page, marp render, CSP, autosave state machine), `backend/src/decks.rs` (extend patterns), `frontend/src/routes/app/deck/[id]/+page.svelte` (the editor being extended)
- **Depends on:** BRIEF-0003 (done — commit `35c14fe`)
- **Language:** code/identifiers/UI copy in English

## Goal
Two phase-2 drag-and-drop capabilities the user asked for ("Drag & Drop เพื่อสร้างสไลด์" / "ลากรูปวางลง editor"): a **slide thumbnail rail** you can drag to reorder slides, and **image upload** by dropping/pasting an image into the editor (stored server-side, inserted as a Markdown image). Everything flows through the existing autosave + Marp preview.

## Scope
### In scope — backend
- **Migration `0005_assets`** (reversible pair) per ERD §5: `id TEXT PK` (uuid), `deck_id TEXT NOT NULL REFERENCES decks(id)`, `filename TEXT NOT NULL` (stored name `<uuid>.<ext>`), `original_name TEXT NOT NULL` (sanitized, for display), `mime TEXT NOT NULL`, `size_bytes INTEGER NOT NULL`, `created_at TEXT NOT NULL`; index `(deck_id, created_at)`.
- **Multipart upload** `POST /api/decks/{id}/assets` (requires `AuthUser`, owner-scoped through the deck → foreign/soft-deleted deck = **404**; `axum::extract::Multipart`, field name `file`):
  - Allowed MIME (allowlist, `[STD]`): `image/png`, `image/jpeg`, `image/gif`, `image/webp`. **SVG is excluded** (active content / XSS vector). Reject others → **415**.
  - Max size **5 MB** per file `[STD]` → over → **413**. The route gets its own `DefaultBodyLimit::max(8 MB)` (multipart overhead) while the 5 MB image cap stays authoritative.
  - Verify the bytes actually match an allowed type by **magic-number sniffing**, not just the declared `Content-Type` — a `.png` full of HTML must be rejected (**415**). Signatures: PNG `89 50 4E 47`, JPEG `FF D8 FF`, GIF `47 49 46 38`, WebP `RIFF????WEBP` (require **both** the `RIFF` prefix at 0 **and** the `WEBP` fourcc at bytes 8–11, not `RIFF` alone).
  - Store at `<DATA_DIR>/assets/<deck_id>/<uuid>.<ext>` (ext derived from the sniffed type, never from user input); create dirs as needed. Insert the row. Return **201** `{id, url, originalName, mime, sizeBytes}` where `url = /assets/<deck_id>/<uuid>.<ext>`.
- **Serve** `GET /assets/{deck_id}/{filename}` (reserved prefix, ADR-0001 — added **outside** `/api`, before the SPA fallback). **Session wiring (critical):** the `SessionManagerLayer` is currently applied only to the inner `/api` router, so a handler on the outer router sees no session → `AuthUser` would 401 every asset. Build the assets serve route as its own sub-router with a **cloned `SessionManagerLayer`** (it is `Clone`; the `SqliteStore` is shared) + `.with_state(state)`, and mount it on the outer router. The origin-guard is not needed there (GET only).
  - Owner-scoped: the `{deck_id}` must belong to the session user and not be soft-deleted, AND an `assets` row must match `(deck_id, filename)`; otherwise **404** (no existence leak, consistent with decks).
  - `{filename}` and `{deck_id}` are both validated against `^[A-Za-z0-9._-]+$` **and explicitly rejected if equal to `.`/`..` or containing a `..` sequence** (defense-in-depth — the primary traversal defense is the `assets` row lookup on `(deck_id, filename)`, which never matches a crafted name since stored names are `<uuid>.<ext>`, but the path is never built from unvalidated input). Serve the file bytes from disk with the stored `mime` as `Content-Type`, **`X-Content-Type-Options: nosniff`** (a polyglot must never be content-sniffed into executable HTML), and `Cache-Control: private, max-age=31536000, immutable` (uuid names are content-stable). Missing file on disk → 404.
  - This route requires a session — so preview `<img src="/assets/…">` works (same-origin cookie) and CSP `img-src 'self'` already permits it. (Share-token access is BRIEF-0008.)
- **Cleanup:** out of scope — soft-deleting a deck leaves its assets on disk for now (documented; a GC brief comes later).

### In scope — frontend
- **Slide segmentation module `src/lib/slides.ts`** — DECK-CORRUPTION-CRITICAL. It must NOT hand-roll a `---`-line rule: a setext H2 underline (`text\n---`), a `---` inside an HTML comment (speaker notes), a `---` inside a 4-space indented code block, and `***`/`___` thematic breaks all make a naive rule desync from what Marp actually renders — and since the rail maps **thumbnails (one Marp SVG per real slide) → reorder indices (slide array)**, any count mismatch drags the WRONG block and silently corrupts the deck. Therefore derive slide boundaries from **the shared Marp instance's own markdown-it tokenizer** (`marp.markdown.parse` → top-level `hr` tokens' `.map` line ranges) so segmentation is identical to Marp's slide split by construction. Detect the leading `---\n…\n---` YAML front matter (closed by `---`/`...`) and preserve it separately. `joinDeck` re-emits with `\n---\n` between slides. **Line endings preserved**: a CRLF deck round-trips as CRLF (operate on LF internally, re-apply the deck's dominant terminator on output) so a reorder never rewrites untouched lines. Hard invariant, tested against Marp: `splitDeck(md).slides.length === renderDeck(md).slideCount` for every fixture. Ships with **vitest unit tests** (see below).
- **`reorderSlides(md, from, to)`** built on the above → returns new markdown with slide block `from` moved to index `to`.
- **Slide rail** (in the editor page): a horizontal, scrollable strip at the top of the workspace, one thumbnail per slide. Thumbnails reuse the marp SVGs (`inlineSVG` emits one `<svg>` per slide) but a bare SVG has no styling — the marp/theme CSS lives in the preview's `marpSheet` constructable stylesheet. So the rail gets **its own shadow root that adopts the SAME `marpSheet` instance** (constructable stylesheets can be adopted by multiple roots) and the per-slide `<svg>` nodes are cloned into it; scale via CSS transform. Each thumbnail is `draggable`; native HTML5 DnD reorders. **Applying a reorder must go through the editor's state path, NOT `setEditorContent` (which sets `applyingRemote=true` and would suppress `currentMarkdown`/`dirty`/autosave/re-render):** dispatch the change, then set `currentMarkdown`, `dirty=true`, `scheduleSave()`, `scheduleRender()` — same shape image-insert uses. Clicking a thumbnail scrolls the preview to that slide and marks it active (active = last-clicked). Reorder disabled while viewing an old revision.
- **Image drop + paste** on the editor pane: dragover shows a drop affordance; on drop (or clipboard paste) of image file(s), each is uploaded via `api.assets.upload`; on success insert `![<alt>](<url>)\n` at the cursor (CodeMirror transaction, then the same state path as reorder — currentMarkdown/dirty/scheduleSave/scheduleRender). **`<alt>` = the original name with Markdown-significant chars escaped/stripped** (`[`, `]`, `(`, `)`, `\`, newlines) so a filename like `array[0].png` can't break the `![](…)` syntax or inject a link. Non-image drops ignored; upload errors surface via the existing status/error affordance; a spinner/disabled state while uploading.
- `lib/api.ts`: `assets.upload(deckId, file): Promise<{id, url, originalName, mime, sizeBytes}>` using `FormData` (no JSON `Content-Type`; the origin-check middleware still applies — same-origin, fine).
- **Responsive:** rail scrolls horizontally on mobile; drop/paste works on desktop (touch DnD not required — `[STD]`); no horizontal page scroll at 375 px.

### Out of scope (later)
- Asset GC / orphan cleanup, per-deck/user storage quota, an asset/media library panel, image resize/optimization, alt-text editing UI, reordering via touch-drag on mobile, caption/figure syntax; present mode (0005); PDF (0006); fonts (0007); sharing incl. share-token asset access (0008)

## Data model
`assets` as above. `decks`/`revisions` unchanged.

## Business rules (the important bits)
- Owner-scoping-as-404 extends to both asset routes (upload + serve), matching decks/revisions.
- Stored filename is always `<uuid>.<ext>` with ext from the **sniffed** type; `original_name` is sanitized (strip path separators + control chars, cap 255) purely for display/inserted alt text.
- MIME allowlist + magic-number check both required (declared type alone is untrusted).
- CSP unchanged (`img-src 'self'` already covers `/assets/`); no new external requests.
- `splitDeck`/`joinDeck` must round-trip losslessly; reorder must never drop, duplicate, or corrupt a slide or the front matter, including decks whose code fences contain `---` lines.

## API / interface surface
`POST /api/decks/{id}/assets` (multipart) and `GET /assets/{deck_id}/{filename}`. All prior routes unchanged.

## Deliverables
- Migration pair `0005_assets`, `multipart` axum feature, assets module (upload + serve, sniffing, owner-scoping) wired into the router (serve outside `/api`), `slides.ts` + vitest tests, slide rail + image drop/paste in the editor, `api.ts` upload method, backend tests below, `package.json` `test:unit` script (vitest) run in the gate.

## Verification / acceptance gate
- **Runs proof:** `docker compose -p deckoala up -d --build` succeeds; health unchanged. **UI flow (desktop + 375 px):** open a deck → slide rail shows a thumbnail per slide → drag a thumbnail to reorder → editor markdown + preview reflect the new order → autosave "Saved" → reload → order persisted; drop a PNG onto the editor → it uploads, a `![…](/assets/…)` line appears, the image renders in the preview (loaded from `/assets/…`, confirmed same-origin in the network log); paste an image from the clipboard → same result.
- curl proof: authed multipart upload of a real PNG → 201 with a `/assets/…` url; `GET` that url → 200 with `image/png`; a non-image (text) upload → 415; a foreign user `GET` of the url → 404; `..`-style filename → 404.
- Tests executed: backend `cargo test` exit 0 (Docker verify stage) with every new behavior covered (upload auth/owner/mime-sniff/size/traversal, serve owner-scoped/mime/nosniff/404, migration roundtrip to v4). Frontend gates run **natively** (matching the existing convention — the Docker verify stage is Rust-only): `npm run check` exit 0 AND `npm run test:unit` (vitest) exit 0, the latter covering `splitDeck`/`joinDeck` round-trip incl. `---` in a code fence, **setext H2 underline (NOT a split), `---` in an HTML comment (NOT a split), `---` in indented code (NOT a split)**, CRLF round-trip, no front matter, single slide; `reorderSlides` (first→last, last→first, no-op, fenced-deck content preserved); and the **`splitDeck` count === `renderDeck` slideCount** invariant across all fixtures.
- Isolation unchanged (only port 8321, `deckoala-*`, all state under `/data`). Run the `pr-review` skill before committing.
