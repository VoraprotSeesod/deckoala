# Brief: Font manager — upload + Google Fonts, served locally

- **ID:** BRIEF-0007
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all, esp. §2 the no-external-CDN invariant + its ONE exception), `docs/design/ARCHITECTURE.md` §4, §5 (ERD `fonts`), §6.5 (font flow), `decisions/ADR-0001-stack.md` (reserved `/fonts/` prefix, `/data`), `docs/briefs/BRIEF-0004-slide-rail-assets.md` (multipart upload + magic-number + owner/serve patterns to mirror), `docs/briefs/BRIEF-0006-pdf-export.md` (print page loads fonts too), `frontend/src/lib/marp.ts`, `frontend/src/routes/app/deck/[id]/+page.svelte`, `backend/src/assets.rs`
- **Depends on:** BRIEF-0006 (done — commit `753051b`)
- **Language:** code/identifiers/UI copy in English

## Goal
The user's "สามารถติดตั้ง font เอง รวมถึงดึง font จากแหล่งต่าง ๆ มาติดตั้งง่าย ๆ": an admin can install fonts into the instance — by uploading a font file, or by fetching a family from Google Fonts (the ONE permitted server-side outbound call) — after which the fonts are served locally and become usable in slides, with viewers never hitting an external CDN.

## Scope
### Backend
- **Migration `0006_fonts`** (reversible pair) per ERD §5 + a `unicode_range` column: `id`, `family`, `weight DEFAULT '400'`, `style DEFAULT 'normal'`, **`unicode_range TEXT NOT NULL DEFAULT ''`**, `filename`, `format` (`woff2|woff|truetype|opentype`), `source` (`upload|google`), `created_at`. Unique index **`(family, weight, style, unicode_range)`** — REQUIRED because Google Fonts css2 returns one `@font-face` per unicode-range subset (Thai, Latin, …) for the same weight; keying only on (family,weight,style) would collide on the 2nd subset and drop the Thai glyphs (the headline use case). Uploads store `unicode_range = ''` (whole font).
- **Fonts are instance-level (Q5 `[STD]`):** install/delete require **admin** (`is_admin`); list + use are available to any signed-in user. Add an `AdminUser` extractor (like `AuthUser` but 403 when not admin).
- **Deps:** `reqwest` (default-features off, `rustls-tls` so certs are bundled — no system-cert dependency; the runtime already has `ca-certificates` but rustls uses webpki roots) for the Google fetch, `ttf-parser` optional (see upload). No other new deps.
- **Endpoints:**
  - `GET /api/fonts` (any authed user) → 200 `[{id, family, weight, style, format, source, createdAt}]` ordered by family, weight.
  - `POST /api/fonts/upload` (**admin**, multipart): fields `family` (required; **CSS-identifier-safe charset only — letters, digits, spaces, hyphens; reject quotes/braces/semicolons/backslash/angle-brackets**, 1–100 chars — because family is emitted verbatim into `font-family` in the public fonts.css), `weight` (validated `^[1-9]00$`, default `400`), `style` (`normal|italic`, default `normal`), `file`. Validate by **magic bytes** (woff2 `wOF2`, woff `wOFF`, ttf `00 01 00 00`/`true`/`ttcf`, otf `OTTO`), size ≤ 5 MB → else 415/413. Store `<DATA_DIR>/fonts/<uuid>.<ext>`, insert row with `unicode_range=''`. Returns 201 the font row. Duplicate (family,weight,style,'') → 409.
  - `POST /api/fonts/google` (**admin**, JSON `{family, weights?: string[]}`): the ONLY external call, and the whole install runs in **one transaction** (any failure rolls back rows; downloaded temp files are cleaned).
    - **Input validation FIRST:** `family` = same CSS-safe charset as upload; each requested `weight` matches **`^[1-9]00$`** (reject 422 otherwise — never interpolate an unvalidated value into the URL); cap the weight count (≤ 18).
    - Build `https://fonts.googleapis.com/css2?family=<url-encoded family>:wght@<validated weights, ';'-joined>` and fetch with a modern-browser `User-Agent` (→ woff2), using a reqwest client with **`redirect::Policy::none()`** (a 3xx to a non-Google host must NOT be followed — it would defeat the host check). Read the CSS body with a **hard byte cap** (stream/abort past ~512 KB) and a request timeout; an overall **install deadline** bounds the whole flow.
    - Parse each `@font-face` block for `font-weight`, `font-style`, `unicode-range`, and the `src: url(...) format('woff2')`. For each: **parse the URL and assert `scheme == https` AND `host == "fonts.gstatic.com"` exactly (parsed host equality, never `contains()`)** — else skip/abort. Download each (client with redirects off, per-file size cap + timeout, capped total files/bytes). Store locally; insert one row per subset (`ON CONFLICT DO NOTHING` for genuine re-installs).
    - Unknown family / Google 404 → 404; network/parse failure → 502.
  - `DELETE /api/fonts/{id}` (**admin**) → 204: remove the row + the file. (In-use decks referencing the family simply fall back — acceptable.)
- **Local serving (no auth — instance-shared, and Chromium/print has no session):**
  - `GET /api/fonts.css` → `text/css`: one `@font-face` per installed font row, `font-display: swap`, `font-family: '<css-escaped family>'`, `font-weight`, `font-style`, **`unicode-range: <range>` when the row's range is non-empty** (so the browser picks the Thai subset for Thai codepoints), `src: url('/fonts/<filename>') format('<format>')`. **`Cache-Control: no-cache`** (must revalidate — distinct from the immutable cache on `/fonts/{filename}`, whose names are uuid-unique) so a freshly installed font is visible on the next load.
  - `GET /fonts/{filename}` (reserved prefix, ADR-0001; outside `/api`, before the SPA fallback): validated filename (`safe_segment`, must match a `fonts` row), served from `<DATA_DIR>/fonts` with the right `Content-Type` (`font/woff2` etc.), `X-Content-Type-Options: nosniff`, long immutable cache. Unknown → 404.
- **CSP:** `font-src 'self' data:` already permits `/fonts/`; no CSP change. The Google fetch is server-side, not a browser request, so CSP does not apply to it.

### Frontend
- **Installed fonts available in every render:** the app document loads `<link rel="stylesheet" href="/api/fonts.css">` (in `app.html`), so the `@font-face`s register at document level and reach the shadow-DOM slide renders (editor preview, present, print — the same mechanism KaTeX/Inter use). The print page (`/print/[id]`) must also carry this link so exported PDFs embed installed fonts. After a successful install/delete in the font manager, **re-fetch the stylesheet link** (swap its href with a cache-busting query) so newly installed fonts appear without a full reload.
- **Font manager UI** `/app/fonts` (linked from the app header): lists installed families (grouped; showing weights/styles/source); an **Upload** form (family + weight + style + file) and a **Google Fonts** form (family name + optional weights) — both admin-only, disabled/hidden with an explanatory note for non-admins; **Delete** per family/variant (admin, confirm). `lib/api.ts` gains `fonts` methods.
- **Using a font in a deck:** installed families are available to any deck via a Marp `style` directive in its front matter, e.g. `style: | \n section { font-family: 'Sarabun', sans-serif; }`. This is standard Marp and needs no new editor machinery. (A one-click per-deck font picker that safely rewrites YAML front matter is **out of scope for this brief** — the review flagged a real deck-corruption risk in a front-matter writer; it is a separate follow-up. This brief delivers install + local serving + availability.)

### Out of scope (later)
- Per-user (non-instance) fonts, font subsetting/optimization, variable-font axis UI, auto-detecting family/weight from the uploaded file's name tables (admin types them), a font-preview specimen, bulk import, non-Google web font sources; sharing (0008)

## Data model
`fonts` as above. Files under `<DATA_DIR>/fonts/`. No change to `decks` (font choice lives in the deck markdown's `style` directive).

## Business rules (the important bits)
- The no-external-CDN invariant (CLAUDE.md §2) holds: the ONLY outbound call is `POST /api/fonts/google` server-side; the CSS/files served to viewers are all same-origin `/fonts/...`.
- SSRF-safe Google fetch: the request URL is built by us to a fixed host; every downloaded font URL is asserted to be on `fonts.gstatic.com`; both requests are size- and time-bounded.
- Admin-gates on install/delete; list/use open to all users.
- Font file validation = magic bytes + size, mirroring the image asset rules.
- `fonts.css` + `/fonts/` are unauthenticated on purpose (shared instance resources, needed by session-less Chromium during PDF export).

## API / interface surface
`GET/POST/DELETE /api/fonts*`, `GET /api/fonts.css`, `GET /fonts/{filename}`. All prior routes unchanged.

## Deliverables
- Migration `0006_fonts`, `AdminUser` extractor, `fonts.rs` (list/upload/google/delete/serve + fonts.css), reqwest dep, router wiring, `app.html` fonts.css link (+ print page), `/app/fonts` UI + editor font picker + `api.ts`, tests below.

## Verification / acceptance gate
- **Runs proof:** `docker compose -p deckoala up -d --build` succeeds; health unchanged. **Browser (desktop + 375 px):** as the admin, open `/app/fonts` → upload a `.woff2` (family e.g. "Sarabun") → it appears in the list and in `GET /api/fonts.css`; **install a Google font by name** (e.g. "Sarabun") → variants download and appear, and the network tab shows the browser making NO external request (only `/api/fonts.css` + `/fonts/...` same-origin); in the editor, pick that family from the Font select → the preview re-renders in that font and it persists after reload; export the deck to PDF → the PDF embeds the chosen font (the render + Chromium already load `/api/fonts.css`). Delete a font → gone from the list + css.
- curl proof: `GET /api/fonts.css` is `text/css` with `@font-face` after an install; `GET /fonts/{file}` serves the bytes with `nosniff`; `POST /api/fonts/upload` and `/google` and `DELETE` return **403 for a non-admin** and work for the admin; a non-font upload → 415.
- Tests executed (Docker verify stage — the Google fetch is NOT hit in cargo test; cover the units): `AdminUser` 403 vs admin; upload magic-byte reject + accept + duplicate 409 + family charset reject + weight `^[1-9]00$` reject; list; `fonts.css` renders `@font-face` (with `unicode-range` for a subset row, css-escaped family) for installed rows and sets `Cache-Control: no-cache`; `/fonts/{file}` serves with `nosniff` + 404s unknown + rejects traversal; delete removes row+file; **the `@font-face`-parser + gstatic-host assertion are unit-tested with a multi-subset sample css2 string (latin+thai) — asserting all subsets parse, non-gstatic/redirect-style hosts are rejected, and weight injection is caught**; migration roundtrip to v5. `cargo test` exit 0; `npm run check` + `npm run test:unit` exit 0; `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` clean.
- Isolation unchanged (only port 8321; all state under `/data`). Run the `pr-review` skill before committing.
