# Brief: Admin settings + root bootstrap + AI slide generation

- **ID:** BRIEF-0010
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (§2 — **this brief amends the outbound-call invariant, see below**; §3 runtime isolation), `docs/design/ARCHITECTURE.md` §4, `backend/src/auth.rs` (register/login/session, first-user-is-admin at 143-160), `backend/src/decks.rs` (`AuthUser` / `AdminUser` extractors), `backend/src/fonts.rs` (`google()` — the existing pattern for a bounded, no-redirect server-side outbound call: timeout, size cap, `read_capped`), `backend/src/export.rs` (semaphore-bounded work), `backend/migrations/0002_users.up.sql`, `frontend/src/lib/components/DeckEditor.svelte`, `frontend/src/lib/i18n.svelte.ts` + `messages.ts` (all new UI copy must be TH+EN)
- **Depends on:** BRIEF-0009 (done — commit `a360bbe`)
- **Language:** code/identifiers in English; all new UI copy bilingual (TH default)
- **User decisions (this brief):** support **both** Anthropic and OpenAI-compatible providers (base URL + model, so local Ollama/LM Studio works), **shipped disabled** until an admin configures it; **one instance API key** held in Admin settings, **never returned to a client**, usable by any signed-in user; seed a **`root` admin account with password `Admin123456@`** (explicitly chosen by the user after the default-credential risk was flagged).

## ⚠️ Invariant amendment (§2)
CLAUDE.md §2 currently locks: *"the only permitted outbound call is the server-side Google Fonts download."* This brief amends it to permit **exactly one more**: a **server-side** call to the **admin-configured LLM endpoint**, made only for an authenticated user's explicit generate request. The spirit is preserved and MUST be verified: **pages served to viewers (deck view, share links, present, print/PDF) still make ZERO external requests** — the browser never talks to the LLM; only the Rust server does. Update CLAUDE.md §2 as part of this brief.

## Goal
An admin can configure the instance (and its AI provider) from an **Admin settings** page, and any signed-in user can press an **AI** button to generate/extend a deck's Marp Markdown from a prompt. The app ships with a working admin account out of the box.

## Scope
### Backend
- **Migration 0008 `settings`** — instance key/value config: `settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL)`. Additive; `down` drops it.
- **Root bootstrap (startup):** if `users` is empty, insert `root` (`is_admin = 1`) with the password from `DECKOALA_ROOT_PASSWORD`, defaulting to **`Admin123456@`**; argon2id-hashed like any other account, and set `settings['root_password_is_default'] = 'true'`. Log a startup WARNING naming the default. Idempotent (never overwrites an existing user). The existing first-user-is-admin path stays as-is (it simply won't trigger once root exists).
- **Change password:** `POST /api/auth/password` `{ currentPassword, newPassword }` — `AuthUser`, verifies the current hash, applies the same strength rules as register, re-hashes, and clears `root_password_is_default` when the changing user is `root`. 204.
- **Default-password warning:** `GET /api/auth/me` (or `/api/instance`) exposes `rootPasswordIsDefault: bool` so the UI can warn until it's changed.
- **Admin settings API** (`AdminUser`):
  - `GET /api/admin/settings` → `{ ai: { enabled, provider, baseUrl, model, apiKeySet, apiKeyLast4 }, rootPasswordIsDefault }`. **The API key itself is NEVER returned.**
  - `PUT /api/admin/settings` → updates the AI block. An **empty/omitted `apiKey` keeps the stored one**; a non-empty value replaces it. Validate `provider ∈ {anthropic, openai}`, `baseUrl` is a parseable http(s) URL, `model` non-empty when enabled.
- **AI generation:** `POST /api/ai/generate` (`AuthUser`) `{ prompt, existingMarkdown? }` → returns `{ markdown }` (Marp Markdown).
  - **503** when AI is disabled or unconfigured; **422** on an empty/oversized prompt (cap ~4 KB).
  - Server-side call only, mirroring `fonts::google()`'s hardening: `redirect::Policy::none()`, explicit timeout (~60 s), **response size cap** via a `read_capped`-style reader, and a **`Semaphore`-bounded** concurrency limit so a burst can't exhaust the instance.
  - Provider adapters: `anthropic` (`/v1/messages`, `x-api-key` + `anthropic-version`) and `openai` (`/v1/chat/completions`, `Authorization: Bearer`) against the configured `baseUrl`.
  - A system prompt constrains output to **valid standalone Marp Markdown** (frontmatter + `---` slide separators) and the response is returned as-is to the client (never executed, never rendered server-side). The key is never logged or echoed.
  - **Note (accepted):** `baseUrl` is admin-set, so pointing it at a LAN/localhost address is a *feature* (local Ollama), not an SSRF bug — but only an admin can set it, and it must never be settable by a non-admin.

### Frontend
- **`/app/admin` settings page** (admin-only; non-admins get a clear "admin only" state, and the nav entry is hidden): the default-password warning + **change-password form**, and the AI section (enable toggle, provider select, base URL, model, API key field that shows `••••1234` when set and only sends a value when the admin types a new one).
- **AI button** in the deck editor top bar: opens a small prompt dialog ("อธิบายสไลด์ที่อยากได้…"), calls `POST /api/ai/generate`, and lets the user **Insert** the result (append) or **Replace** the deck — never silently overwriting. Busy/error states; hidden (or disabled with a hint) when AI is off.
- All new copy in `messages.ts` (TH + EN), theme variables only (dark-mode clean).

### Out of scope (next brief)
- **MCP server** (`/mcp` over HTTP + per-user API tokens) → **BRIEF-0011**.
- Streaming/token-by-token generation; per-user keys; usage accounting/quotas; image generation.

## Business rules
- The AI feature is **OFF by default**; a fresh instance never calls out until an admin enables and configures it.
- The API key is **write-only** across the whole API surface (never in any GET, never logged, never in an error body).
- Only `AdminUser` may read or write settings; `POST /api/ai/generate` requires a signed-in user.
- **Viewer-facing pages keep making zero external requests** — verify in the browser network tab on a share link + present + PDF export.
- Root seeding is idempotent and must never reset an existing account's password.

## Deliverables
Migration 0008 · root bootstrap + `POST /api/auth/password` · `settings` module (get/put, key write-only) · `ai` module (provider adapters, bounded outbound call) · router wiring + `AppState` (settings cache/semaphore) · backend tests (admin-only settings; key never returned; empty-key-keeps-existing; AI 503 when disabled; change-password happy/wrong-current; root seed idempotent + flag cleared; migration roundtrip) · `/app/admin` page + editor AI dialog + TH/EN copy · **CLAUDE.md §2 amended**.

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): `docker compose -p deckoala up -d --build` healthy + `/api/health`; `cargo test` (visible pass count, exit 0) + `cargo fmt --check` + `cargo clippy -- -D warnings`; `npm run check` (0/0) + vitest + build; `pr-review` PASS before commit.
- Runtime: a **fresh volume** boots with `root` able to sign in using `Admin123456@`, and the UI shows the default-password warning; changing the password clears it and the old password stops working. A non-admin cannot reach `/api/admin/settings` (403) and never sees the key. With AI disabled, the generate endpoint 503s and the button is hidden. After an admin configures a provider, the AI button produces Marp Markdown that renders as slides. **A share link, present mode, and a PDF export still show zero external requests in the network tab.**
- Adversarial review (2 lenses — implementability + security/secrets) before AND after coding, findings folded, as with prior briefs.

## Folded review findings (pre-code adversarial review, 2026-07-21 — 56 agents)
**Root bootstrap**
1. **Seed in `main.rs` startup, NEVER in `init_db()`.** Every integration test calls `init_db()` and then relies on "the first `signup()` is the admin" (fonts/shares/decks suites). Seeding inside `init_db` would give every test DB a pre-existing user and silently break that assumption across the suite. Seed after `init_db` in `main.rs` only.
2. **Only set `root_password_is_default` when the seed actually used the built-in default** — an operator supplying `DECKOALA_ROOT_PASSWORD` must not get a permanent false warning. Set it at seed time based on which password was used, and clear it on change.
3. **`DECKOALA_ROOT_PASSWORD` must be plumbed through** `compose.yml` (its `environment:` is an explicit 3-key allow-list, so a new var is silently dropped) **+ `.env.example` + the README env table** — otherwise the override ships broken. Acceptance adds a fresh-volume boot WITH the override set, verifying `Admin123456@` is then rejected.
4. Seeding is idempotent and must not lock out account creation when `DECKOALA_ALLOW_SIGNUP=false` (root IS the account; verify the existing first-run insert path still behaves).

**Secrets / disclosure**
5. **`rootPasswordIsDefault` goes ONLY on session-gated `GET /api/auth/me`** — NOT on `/api/instance`, which is anonymous (`lib.rs:229`, routed at `:317`, already fetched by the signed-out login page). Anonymous exposure would be a mass-scannable "this box still has the default root password" beacon. Add a test asserting the anonymous `/api/instance` body contains **no such key at all** (absent, not `false`).
6. Adding a field to the shared `UserDto` also changes the **register/login** response shapes — do it deliberately (extend the `/me` response specifically rather than leaking the flag into every auth response).
7. **Give non-admins an authorized `aiEnabled` signal** (e.g. on `/me`) so the UI can decide whether to show the AI button **without** exposing `baseUrl`/`model` (which can name internal hosts). Never return provider config to a non-admin.
8. The settings API needs an explicit **"remove key"** affordance — "empty keeps existing" alone leaves an admin unable to ever clear a key.

**AI endpoint hardening**
9. **Per-user rate limiting**, not just a global semaphore: any signed-in user could otherwise burn the instance's provider budget. Cap `prompt` (~4 KB) **and `existingMarkdown`** (decks reach 1 MB — an uncapped echo is both a cost and a payload bomb), and always send an explicit **`max_tokens`**.
10. **The AI button must NOT render on anonymous share-edit links.** `DeckEditor` is shared by the owner route and `/s/[token]`; pass AI capability as a prop that only the owner route sets.
11. Specify **base-URL join semantics** (store the origin without a version segment; append the provider path exactly once — avoid `/v1/v1/...`) and pin the **Anthropic** adapter details (`anthropic-version` header value, required `max_tokens`).

**Mechanics**
12. **`reqwest` has no `json` feature** here (`default-features = false, features = ["rustls-tls"]`) — serialize with `serde_json::to_string` + set `content-type`, and parse the capped response with `serde_json::from_slice`. No `Cargo.toml`/lock change.
13. `hash_password`/`verify_password`/`argon2()` are private to `auth.rs` — put the change-password handler **in `auth.rs`** to reuse them. Any `AppState` addition must update **both** literal constructors (`main.rs` and `backend/tests/common/mod.rs`).
14. **Password change should invalidate the user's other sessions** (tower-sessions) so a stolen session doesn't survive a remediation password change.

## Folded post-code review (2026-07-21 — 30 agents, 7 confirmed)
Fixed:
- **Enabling AI never revealed the editor's AI button** (the `/app` layout load caches `user.aiEnabled` and re-runs only on a full page load) — `saveAi` now calls `invalidateAll()`.
- **Unbounded wait on the AI semaphore** (the reqwest timeout starts only *after* the permit is granted, so a backlog could stall other users indefinitely) — the acquire is now time-boxed, returning 503 "queue busy".
- **Settings write was not transactional and flipped `ai_enabled` first** — now one transaction with the enable flag written **last**, so a partial failure can never leave AI on with a half-written provider/key.
- **`rootPasswordIsDefault` was returned to every signed-in user** — now admin-only (only someone who can act on it learns it).
- **Change-password length check counted bytes while register counts characters** (a 3-glyph Thai password would pass one and fail the other) — both now count characters.
- **`strip_code_fence` dropped the opening line of an unclosed fence** — it now strips only a properly closed wrapper.
- **Oversized deck context returned 422** even though the editor sends the whole deck by default — the context is now truncated on a char boundary instead of rejecting the request.

Accepted limitations (documented, not fixed):
- **Other sessions survive a password change.** `session.cycle_id()` rotates the caller's session; tower-sessions offers no per-user session enumeration, so a full fix needs a session-epoch column checked in `AuthUser` (which today does no DB read). Tracked as deferred hardening.
- **OpenAI reasoning models reject `max_tokens`** (they expect `max_completion_tokens`). The adapter sends `max_tokens`, which is what OpenAI chat models and the OpenAI-compatible servers we target (Ollama, LM Studio, vLLM) expect; pointing at an o-series model surfaces a clear "provider rejected the request" error. Per-model parameter negotiation is out of scope here.
