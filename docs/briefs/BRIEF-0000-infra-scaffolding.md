# Brief: Infra & scaffolding — the repo runs

- **ID:** BRIEF-0000
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all), `docs/design/ARCHITECTURE.md` §2 §3 §4 §7, `decisions/ADR-0001-stack.md`, `decisions/ADR-0003-pdf-export.md` (only for why Chromium is in the image)
- **Depends on:** — (first brief)
- **Language:** code/identifiers/comments in English; UI copy in English (Thai i18n is a later brief)

## Goal
A runnable skeleton: `docker compose -p deckoala up -d --build` brings up one container where the Rust binary serves a branded landing page and a health endpoint, with SQLite migrations infrastructure and Chromium present in the image. No product features yet.

## Scope
### In scope
- Repo layout per ARCHITECTURE §2: `frontend/` (SvelteKit, Svelte 5 + TS, `adapter-static` with SPA fallback), `backend/` (cargo crate `deckoala-server`), `compose.yml`, `Dockerfile`, `.env.example`, `README.md`, `.gitignore`, `rust-toolchain.toml`; `git init` + initial commit(s).
- **Backend:**
  - Axum server binding `DECKOALA_BIND` (default `0.0.0.0:8080`).
  - `GET /api/health` → `200 {"status":"ok","version":"<cargo pkg version>","db":"ok","chromium":<bool>}`.
    - `db`: `"ok"` iff `SELECT value FROM meta WHERE key='schema_seeded'` returns `'1'` (proves migrations ran); otherwise `"error"` and overall `status` is `"degraded"`.
    - `chromium`: `true` iff `CHROME_BIN` is set **and** a regular file exists at that path; `false` when unset — merely checking the env var is not acceptable.
  - Static file service for the built SPA from `DECKOALA_STATIC_DIR` (default `./static`; the Dockerfile copies the SvelteKit build there — `cargo run` in dev serves no SPA, Vite covers it). `index.html` fallback applies only to routes outside the reserved backend prefixes `/api/`, `/assets/`, `/fonts/` (ADR-0001 contract); unknown `/api/*` paths return JSON 404, never the SPA.
  - Bootstrap `DECKOALA_DATA_DIR` (default `/data`): create dir, open SQLite (`deckoala.db`, WAL), run sqlx migrations; migration `0001_baseline` creates only a `meta` table (`key TEXT PRIMARY KEY, value TEXT`) with row `schema_seeded=1`. Product tables come in later briefs.
  - Config from env only; no config files.
- **Frontend:** landing page `/` — logo (`assets/brand/logo.svg`, copied into `frontend/static/`), name "Deckoala", one-line tagline "Markdown presentations, self-hosted.", background `#F8F8FF`, text `#0B1215`; bundled fonts Inter + Noto Sans Thai via `@fontsource` packages (zero external requests — check the network tab). Global CSS custom properties `--dk-bg: #F8F8FF; --dk-ink: #0B1215;`. Favicon = the logo SVG.
- **Docker:** multi-stage Dockerfile (pinned tags): Node LTS stage builds frontend → Rust stage builds release binary → `debian:bookworm-slim` runtime with `chromium` installed, `CHROME_BIN=/usr/bin/chromium`, non-root user, SPA build baked into the image, `/data` volume. In the runtime stage, create `/data` and `chown` it to the app user **before** the `USER` instruction, so the named volume is initialized with ownership the non-root process can write to (root-owned volume + SQLite EACCES crash-loop is the classic failure here). `compose.yml` exactly per ARCHITECTURE §7 (project `deckoala`, port `${DECKOALA_PORT:-8321}:8080`).
- **Dev workflow (document in README):** `cargo run` (backend on 8080) + `npm run dev` (Vite on its own port, proxy `/api` → 8080); plus the self-host section: compose usage, `.env`, pointing a reverse proxy at the port for `deckoala.dimenshade.com` (DNS/proxy config itself is the user's infra, not this repo).
- Backend tests: integration tests (axum + tower test utilities) — (1) `/api/health` returns 200, `status == "ok"`, `db == "ok"` against a real migrated temp SQLite DB (proves the migration infrastructure actually runs, not stubbed); (2) unknown `/api/*` path returns JSON 404, not the SPA fallback.

### Out of scope (later)
- Auth, decks, editor, preview, present, PDF logic (only Chromium *presence*), fonts manager, sharing, i18n, CI pipeline, published GHCR image.

## Data model
Only the `meta` table via migration `0001_baseline` (see In scope). No product entities yet — ERD lives in ARCHITECTURE §5 for later briefs.

## Business rules (the important bits)
- Single container serves everything; all persistent state strictly under `/data` (ADR-0001 contract).
- All API routes under `/api/`; SPA fallback must never shadow `/api/*` (ADR-0001 contract).
- No external network requests from the served pages (fonts bundled locally) — self-host friendliness `[SRC]`.
- Brand colors exactly `#F8F8FF` / `#0B1215` `[SRC]`.

## API / interface surface
- `GET /api/health` → `{"status":"ok","version":string,"chromium":boolean}` (the only endpoint in this brief).

## Deliverables
- All files above, building green; initial git history on `main`.
- `README.md` with: what Deckoala is, self-host quickstart (compose), dev quickstart, env table.

## Verification / acceptance gate
- **Runs proof:** `docker compose -p deckoala up -d --build` succeeds on Docker Desktop (Windows host); `curl http://localhost:8321/api/health` → 200 with `"status":"ok"`, `"db":"ok"` and `"chromium":true`; loading `http://localhost:8321/` in a real browser shows the landing page with the koala logo and brand colors (screenshot or visual confirmation, not just HTTP 200), and the landing also renders correctly at a mobile viewport (~375px wide — responsive invariant, ARCHITECTURE §5); `docker compose -p deckoala down` leaves no deckoala containers/networks behind (volume may persist).
- Tests executed: `cargo test` exit 0 with visible pass count (≥2 tests per Deliverables); `npm run check` exit 0. (Host has no local Rust toolchain → running the cargo gates inside the Rust build image / a Dockerfile verify stage is acceptable and equivalent.)
- Lint/format: `cargo fmt --check` and `cargo clippy -- -D warnings` clean.
- Isolation: only port 8321 bound on the host; all resources named with the `deckoala` prefix.
- Run the `pr-review` skill before committing.
