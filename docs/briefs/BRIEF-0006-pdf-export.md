# Brief: PDF export via headless Chromium

- **ID:** BRIEF-0006
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all), `decisions/ADR-0003-pdf-export.md` (the whole contract), `docs/design/ARCHITECTURE.md` §4 (backend PDF), §6.4 (export flow), `docs/briefs/BRIEF-0004-slide-rail-assets.md` (asset serving to extend), `docs/briefs/BRIEF-0005-present-mode.md` (the print page reuses the same Marp render pipeline), `frontend/src/lib/marp.ts`, `backend/src/lib.rs` (router + AppState), `backend/src/assets.rs` (serve auth to extend), `Dockerfile` (Chromium already installed, `CHROME_BIN=/usr/bin/chromium`)
- **Depends on:** BRIEF-0005 (done — commit `19b93a9`)
- **Language:** code/identifiers/UI copy in English

## Goal
The user's "Export เป็น PDF": a button that downloads a PDF matching the on-screen slides 100% — including Thai fonts, KaTeX math, images, and the deckoala theme — by driving the container's headless Chromium to print a token-authorized print view of the app itself (ADR-0003). No second renderer.

## Scope
### Backend
- **Dependencies:** `chromiumoxide` (CDP driver; tokio runtime feature), `hmac` + `sha2` (stateless print tokens), `futures` (chromiumoxide handler). No new DB table, no migration.
- **`AppState` additions** (all `#[derive(Clone)]`-safe): `print_secret: [u8; 32]` (random at process start; tests use a fixed value), `local_addr: String` (**loopback address Chromium dials — derived from `DECKOALA_BIND` by swapping the host for `127.0.0.1` and keeping the port**, so a non-default `DECKOALA_BIND` port still works; NOT hardcoded 8080), `export_sem: Arc<tokio::sync::Semaphore>` (constructed once, permits = 2; tests = 2).
- **Stateless print token:** Token = `"<deck_id>.<expiry_unix>.<hex hmac_sha256(secret, deck_id|expiry)>"`. `mint_print_token(secret, deck_id, ttl=120s)` and `verify_print_token(secret, token) -> Option<deck_id>` (parse, **constant-time** HMAC compare, reject if expired). No storage/cleanup. (An in-flight export does not survive a process restart — the secret is regenerated — which is acceptable for a 120 s bearer used within one request.)
- **Endpoint `POST /api/decks/{id}/export/pdf`** (session, owner-scoped → foreign/soft-deleted deck = 404 **before any Chromium work**, so the owner/auth tests never launch a browser):
  - **Cache first:** `sha256(markdown + '\n' + theme)` → `<DATA_DIR>/exports/<deck_id>/<hash>.pdf`; if present, stream it (no Chromium). Else generate, store, stream. Repeat exports of an unchanged deck are instant.
  - **Generate:** **acquire an owned semaphore permit FIRST** (so cancellation releases it and the token TTL never burns in the queue), THEN mint the token, THEN drive Chromium (helper `render_pdf`). Launch the browser with `CHROME_BIN`, headless, `--no-sandbox --disable-gpu --disable-dev-shm-usage`, **and `--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1`** so the loopback-only contract is enforced at the browser layer (external hosts can't even resolve — real SSRF defense, not just CSP; ADR-0003). Set cookie `deckoala_print=<token>` for `127.0.0.1` via CDP; navigate to `http://<local_addr>/print/<id>`; wait until `window.__DECKOALA_PRINT_READY === true` (poll, and wrap the WHOLE drive in `tokio::time::timeout(~20s)` → error, never hang); `Page.printToPDF` with `print_background=true`, zero margins, `prefer_css_page_size=true`.
  - **Guaranteed teardown (DoS fix):** the browser handle lives in an RAII guard that on **every** exit path (timeout, CDP/nav error, panic, cancellation) calls `browser.close().await` AND kills/reaps the OS child AND aborts the spawned CDP handler task — the semaphore counts permit holders, not orphaned processes, so a leaked Chromium per failed export would defeat it.
  - **Response:** `200 application/pdf`, `Content-Disposition: attachment` from the deck title via the **shared `content_disposition(title, ext)` helper (factored out of decks.rs, parameterized by extension — pass `pdf`; `.md` export keeps passing `md`)**, body = the PDF bytes. Chromium/CDP failure → `500` JSON error (never a broken/empty PDF).
- **Endpoint `GET /api/print/{id}`** (read-only, **print-token cookie only**, never the session): reads the `deckoala_print` cookie, `verify_print_token`, and requires the token's deck_id to equal `{id}` AND the deck to exist + not be soft-deleted; returns `{title, markdown, theme}`. Invalid/missing/expired token or mismatched deck → 404.
- **Extend `GET /assets/{deck_id}/{filename}`** so a valid `deckoala_print` cookie whose token deck_id matches `{deck_id}` authorizes the read (in addition to the session owner) — otherwise images 404 inside Chromium and the PDF is imageless. The `X-Content-Type-Options: nosniff` + validation stay.
- **CSP note:** the `/print` responses must keep `img-src 'self'` etc. (unchanged CSP is fine; the print page loads only same-origin assets).

### Frontend
- **Route `/print/[id]`** (no `/app` layout; loaded ONLY by Chromium with the print cookie):
  - `+page.ts` load: `fetch('/api/print/{id}')` (the cookie rides along); on failure `error(404)`. SSR already off.
  - `+page.svelte`: render the deck once via `renderDeck`; lay out **every slide as its own full-bleed page** — each `svg[data-marpit-svg]` wrapped in `.marpit` inside a `.page` sized to the slide's intrinsic dimensions (from the SVG `viewBox`), `break-after: page`, no gaps/margins/chrome, white slide background. Inject a `<style>` (via a Svelte `{@html}` of a **non-user, self-authored** string only — the slide CSS goes through the constructable-stylesheet/adopted path, never user content) or a scoped style setting `@page { size: <w>px <h>px; margin: 0 }` from the measured slide size. Import the KaTeX CSS here too (fonts must be embedded in the PDF).
  - **Readiness signal:** after the DOM is painted, await **both** `document.fonts.ready` AND every image finishing (`Promise.all` over `img` elements: `img.complete ? ok : (img.decode?.() ?? onload/onerror)`) — `document.fonts.ready` does NOT wait for `/assets` images, so printing too early yields an imageless PDF. Then a `requestAnimationFrame` tick, then set `window.__DECKOALA_PRINT_READY = true`. This is the flag `render_pdf` polls.
- **Export button:** the editor top bar's/dashboard's Export currently links to `.md`. Add a **"PDF"** action (editor top bar + dashboard card) that POSTs to `/api/decks/{id}/export/pdf` and triggers a download of the returned blob (fetch → blob → object URL → `<a download>`), with a "Generating…" state and error surfacing (PDF generation is slow — seconds).

### Out of scope (later)
- Per-slide/range export, PDF/A, page numbers/branding overlays, background pre-generation/queue, export of the presenter notes, PPTX/HTML export; share-token PDF (0008); font manager (0007)

## Data model
None (stateless tokens, filesystem PDF cache under `/data/exports`). No migration.

## Business rules (the important bits)
- PDF parity is the whole point: the print page uses the SAME `marp.ts` render as editor/preview/present (ADR-0002). No server-side slide rendering.
- Owner-scoping: the export POST is session-owner-scoped (404 like all deck routes). The print-token path is deck-scoped and short-lived; a token for deck A can never read deck B or its assets.
- Chromium may open only `127.0.0.1` URLs (ADR-0003 durable contract); it never receives a user-supplied URL.
- All generated PDFs live under `/data/exports` (deletable cache); nothing leaves `/data`.
- Filename/Content-Disposition sanitation reuses the BRIEF-0002 rule (control-char-free title; ASCII fallback + `filename*`).

## API / interface surface
`POST /api/decks/{id}/export/pdf` (session), `GET /api/print/{id}` (print-cookie), extended `GET /assets/...` auth. New client route `/print/[id]`.

## Deliverables
- `export.rs` (token mint/verify, export handler + Chromium `render_pdf` + cache, print-data handler), asset-serve auth extension, `AppState.print_secret` + wiring, Chromium semaphore, shared `content_disposition` helper, `/print/[id]` route + PDF export buttons, tests below.

## Verification / acceptance gate
- **Runs proof:** `docker compose -p deckoala up -d --build` succeeds; health unchanged. **The real Chromium path (only runnable in the runtime image, which has Chromium — the cargo build stage does NOT):** authenticated `POST /api/decks/{id}/export/pdf` on a deck with a Thai heading, a `$$…$$` equation, and an uploaded image returns `200 application/pdf`, a non-trivial body starting with `%PDF-`, and a filename derived from the title; opening the PDF shows the slides with the deckoala theme, Thai text, rendered math, and the image present (visual confirmation). A second identical export is served from cache (fast, still `%PDF-`). A foreign user → 404. **In a browser:** the editor's "PDF" button downloads that PDF.
- Tests executed (Docker verify stage, NO Chromium there — so cover the non-Chromium units): `verify_print_token` accepts a freshly minted token and rejects an expired one, a tampered one, and one for a different deck; `GET /api/print/{id}` returns the deck with a valid print cookie and 404 without/wrong; `GET /assets/...` is authorized by a matching print cookie and 404 by a print cookie for another deck; the export POST is 401 unauthenticated and 404 for a foreign deck **before** any Chromium launch (owner check precedes browser work). `cargo test` exit 0; `npm run check` + `npm run test:unit` exit 0; `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` clean.
- Isolation unchanged (only port 8321; all state under `/data`). Run the `pr-review` skill before committing.
