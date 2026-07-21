# Brief: Auth & users — multi-user foundation

- **ID:** BRIEF-0001
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all, esp. §2 stack + §3 runtime rules), `docs/design/ARCHITECTURE.md` §4 (backend), §5 (ERD `users`), §6 (invariants), `decisions/ADR-0001-stack.md` (reserved prefixes, /data contract), `docs/briefs/BRIEF-0000-infra-scaffolding.md` (what already exists)
- **Depends on:** BRIEF-0000 (done — commit `810633b`)
- **Language:** code/identifiers/comments/UI copy in English

## Goal
Real multi-user accounts: register, login, logout with cookie sessions; the SPA gains a login/register page and a guarded `/app` shell showing the signed-in user. This is the foundation the deck CRUD (BRIEF-0002) and sharing (BRIEF-0008) build on.

## Scope
### In scope
- **Migration `0002_users`** (reversible pair) creating `users` per ERD §5.
- **Password hashing:** argon2id via the `argon2` crate (unique random salt per hash); hashing/verification run inside `tokio::task::spawn_blocking` (argon2 is CPU-bound, do not block the runtime).
- **Sessions:** `tower-sessions` + `tower-sessions-sqlx-store` (SqliteStore on the existing pool; run the store's `migrate()` at boot). Cookie: HttpOnly, SameSite=Lax, `Secure` off by default (TLS terminates at the user's reverse proxy — document in README); expiry: 30 days on inactivity `[STD]`. Session id is cycled on login/register (fixation defense).
- **CSRF protection = origin check `[STD]`:** middleware on all mutating `/api` methods (POST/PUT/PATCH/DELETE). Exact algorithm (post-review): byte-compare the **authority** of `Origin` (everything after `scheme://`, no port normalization — so `https://example.com` matches bare Host `example.com`) case-insensitively against (a) the raw `Host` header, OR (b) the authority of `DECKOALA_PUBLIC_URL` when set (covers proxies that rewrite Host). `Origin: null` or unparsable → 403. Absent Origin (curl, native clients) passes — SameSite=Lax is the backstop for that case. ARCHITECTURE §4 already reflects this decision. **Dev-proxy caveat:** Vite's string-shorthand proxy implies `changeOrigin: true`, which makes Host ≠ Origin and would 403 every dev-mode mutation — `vite.config.ts` must use the object form with `changeOrigin: false`.
- **Endpoints** (all JSON, camelCase fields):
  - `POST /api/auth/register` `{username, password}` → 201 `{user}` + session. 403 when signup disabled (see rules), 409 duplicate username, 422 invalid input.
  - `POST /api/auth/login` `{username, password}` → 200 `{user}` + session. 401 uniform `{"error":"invalid username or password"}`.
  - `POST /api/auth/logout` → 204, session destroyed.
  - `GET /api/auth/me` → 200 `{user}` with session, else 401.
  - `GET /api/instance` → 200 `{allowSignup, hasUsers}` (drives the login page mode; no auth required).
  - `user` JSON = `{id, username, isAdmin, createdAt}`.
- **Config:** `DECKOALA_ALLOW_SIGNUP` (default `true`), `DECKOALA_SECURE_COOKIE` (default `false`; sets the cookie `Secure` attribute for HTTPS deployments), `DECKOALA_PUBLIC_URL` (optional; its authority becomes an additional allowed origin) — read in `main` into `AppState` (tests construct the state directly). Pass all three through `compose.yml` + document in `.env.example` and README env table.
- **Frontend:**
  - `/login` page — one form, three modes driven by `GET /api/instance`: first-run ("Create the first account" when `hasUsers=false`), sign-in, register (tab/toggle, hidden when `allowSignup=false`). Brand styling, responsive.
  - `/app` — guarded shell (SPA load fetches `/api/auth/me`; 401 → redirect `/login`): header with logo + username + Logout button, placeholder body ("Your decks arrive in the next build phase."). Landing `/` button "Open app" → `/app`.
  - Small `src/lib/api.ts` fetch wrapper (JSON, same-origin credentials, typed errors) — later briefs reuse it.
- **Backend tests** (extend `tests/`, same real-SQLite pattern as BRIEF-0000):
  1. register first user → 201, `isAdmin=true`, Set-Cookie present; `/api/auth/me` with that cookie → 200 same user.
  2. second register → `isAdmin=false`.
  3. duplicate username → 409 (case-insensitive: `Koala` vs `koala`).
  4. login with wrong password → 401; unknown username → same 401 body.
  5. logout → 204; `me` with the old cookie → 401.
  6. `allow_signup=false` state: register → 403, **except** when zero users exist (first-run bootstrap always allowed `[STD]`).
  7. mutating request with foreign `Origin` header → 403; same-origin `Origin` → passes (origin-check middleware).
  8. migration round-trip: `Migrator::undo` back to version 1 → `users` table gone; re-migrate → back. (Proves down-migrations actually run — the debt noted in SESSION_STATE 2026-07-21.)
  9. origin-matching unit tests: `https://example.com` vs bare Host `example.com` → pass; `Origin: null` → reject; authority matching `DECKOALA_PUBLIC_URL` with mismatched Host → pass.
  10. two **concurrent** first registrations (different usernames) → exactly one admin row.
  11. password > 128 bytes → 422 (rejected before hashing).

### Out of scope (later)
- Password reset / change, profile editing, user administration UI (admin can't manage others yet)
- Rate limiting / lockout (hardening brief later), email anything (accounts are username-only per ERD `[STD]`)
- Deck CRUD (BRIEF-0002), share links (BRIEF-0008)

## Data model
`users` exactly per ARCHITECTURE §5 ERD: `id TEXT PK` (uuid v4), `username TEXT NOT NULL UNIQUE COLLATE NOCASE`, `password_hash TEXT NOT NULL`, `is_admin INTEGER NOT NULL DEFAULT 0`, `created_at TEXT NOT NULL` (UTC RFC3339). Plus the `tower_sessions` table owned by the session store's own migrator.

## Business rules (the important bits)
- Username: `^[a-z0-9_-]{3,32}$` after lowercasing input; password: ≥ 8 chars **and ≤ 128 bytes** (rejecting oversized input BEFORE hashing — unbounded input into argon2 is a DoS vector; axum's default 2 MB body limit stays on) `[STD]`.
- **Argon2id parameters pinned explicitly** (not left to crate defaults): `m=19456 KiB, t=2, p=1` (OWASP minimum), centralized in one constructor function so they can be raised later `[STD]`.
- **First registered user becomes admin** (`is_admin=1`) `[STD]` — self-host bootstrap; also exempt from the signup-disabled check. **Race-safety required:** admin assignment and the bootstrap exemption must be decided atomically in a single INSERT statement (subquery on `COUNT(*)`), never check-then-act — two concurrent first registrations must yield exactly one admin, and with signup disabled the losing insert affects 0 rows → 403 (uniformly, regardless of whether the probed username exists).
- Login errors are uniform (no user enumeration on the login path) **including timing**: when the username is unknown, verify against a fixed dummy argon2id hash (same parameters) so both failure branches do equivalent CPU work. (Register inherently reveals taken usernames via 409 — accepted, noted as a driver for the future rate-limiting/hardening brief.)
- All timestamps UTC RFC3339 (CLAUDE.md §2 invariant).
- Reserved prefixes/SPA fallback behavior from BRIEF-0000 unchanged.

## API / interface surface
See Scope. Nothing outside `/api/auth/*` and `/api/instance`; `/api/health` unchanged.

## Deliverables
- Migration pair `0002_users`, backend modules (auth handlers, origin-check middleware, session wiring), frontend pages/lib above, compose + `.env.example` + README updates (ARCHITECTURE §4 already reflects the CSRF decision — no doc change needed), tests 1–11 executed.

## Verification / acceptance gate
- **Runs proof:** `docker compose -p deckoala up -d --build` succeeds; `/api/health` still returns 200 with `status="ok"`, `db="ok"`, `chromium=true` (field assertions — the `version` field from BRIEF-0000 is unchanged); **UI flow exercised in a real browser**: register → lands in `/app` showing the username → logout → login again — desktop **and** ~375px mobile viewport (responsive invariant).
- curl proof: register 201 + cookie → `me` 200 → logout 204 → `me` 401.
- Tests executed: `cargo test` exit 0, ≥ 11 passing tests visible; `npm run check` exit 0; `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` clean (Docker verify stage acceptable per BRIEF-0000 precedent).
- Isolation: unchanged (only port 8321; `deckoala-*` resources).
- Run the `pr-review` skill before committing.
