# Brief: MCP server — connect external AI clients

- **ID:** BRIEF-0011
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (§2 invariants — note MCP is **inbound**, it adds NO outbound call; §3 runtime isolation), `backend/src/decks.rs` (owner-scoped CRUD + the revision-snapshot transaction — the single source to reuse; note every `*_core` currently returns an HTTP `Response`), `backend/src/auth.rs` (argon2 helpers, session extractor pattern), `backend/src/shares.rs` (the closest precedent: bearer-token auth resolving to a scoped identity, uniform 404s, revocation), `backend/src/lib.rs` (router layering: `/api` carries the session layer + `same_origin_guard`; `/assets` and `/fonts` are mounted OUTSIDE it), `backend/migrations/0008_settings.up.sql` (latest is 0008), `frontend/src/lib/messages.ts` (all new copy TH+EN)
- **Depends on:** BRIEF-0010 (done — commit `6fe70c0`)
- **Language:** code/identifiers in English; all new UI copy bilingual (TH default)
- **User decision:** remote **MCP over HTTP** with **per-user revocable API tokens**, scoped to that user's own decks, so Claude Desktop/Code (or any MCP client) can connect over the network.

## Goal
The user's "ต้องมี mcp ให้เชื่อมด้วย เผื่อจะใช้ AI นอกโปรแกรมเชื่อม": a user mints an API token in Deckoala, points an MCP client at `https://<host>/mcp`, and that client can then list, read, create and edit **that user's** decks — with no browser session and no access to anyone else's data.

## Scope
### Backend
- **Migration 0009 `api_tokens`** — `id TEXT PK`, `user_id TEXT NOT NULL (FK users.id)`, `name TEXT NOT NULL`, `token_hash TEXT NOT NULL UNIQUE`, `created_at TEXT NOT NULL`, `last_used_at TEXT`, `revoked_at TEXT`; index on `user_id`. Additive; `down` drops it.
- **Token model:** value is `dko_` + 32 random bytes hex (256-bit). Only a **SHA-256 hash** is stored — these are high-entropy random secrets, so a fast hash is correct (argon2 is for low-entropy passwords) and lookup is a single indexed hash match. The plaintext is shown **exactly once** at creation and never again (no endpoint can return it). Active = `revoked_at IS NULL`.
- **Owner API** (`AuthUser`, i.e. session — a token must not be able to mint more tokens):
  - `POST /api/tokens` `{ name }` → 201 `{ id, name, token, createdAt }` — the **only** response ever containing the plaintext.
  - `GET /api/tokens` → the caller's tokens: `{ id, name, createdAt, lastUsedAt, revokedAt }` (never the value/hash).
  - `DELETE /api/tokens/{id}` → 204, revoke; **double-scoped** by `id AND user_id` so one user can never revoke another's token (mirrors the BRIEF-0008 share-revoke lesson).
- **`McpUser` extractor:** reads `Authorization: Bearer <token>`, SHA-256s it, looks up an **active** token, yields the owning `user_id`; best-effort `last_used_at` touch. Any failure → **401** with a plain JSON body and deliberately **no** `WWW-Authenticate` header (see folded finding 1). Constant-shape errors; never echo the token, never log it.
- **`POST /mcp`** — mounted **outside `/api`** (like `/assets` and `/fonts`) so it carries no session layer: pure Bearer auth, no cookies, no CSRF surface. JSON-RPC 2.0, hand-rolled (the method set is small; adding an MCP SDK dependency is not warranted):
  - `initialize` → `{ protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "deckoala", version } }`; echo the client's `protocolVersion` when we recognise it, else our default.
  - `ping` → `{}`. Notifications (`notifications/initialized`, any message with **no `id`**) → **no response body** (HTTP 202), never a JSON-RPC reply.
  - `tools/list` → the tool schemas below. `tools/call` → dispatch; tool results use the MCP content shape `{ content: [{ type: "text", text }], isError? }`.
  - Unknown method → JSON-RPC error `-32601`; malformed JSON → `-32700`; bad params → `-32602`. Transport-level auth failures stay HTTP 401 (not JSON-RPC errors).
- **Tools (all scoped to the token's user):** `list_decks`, `get_deck { deckId }`, `create_deck { title, markdown? }`, `update_deck { deckId, title?, markdown? }`. A deck the user doesn't own is indistinguishable from a missing one (uniform not-found), matching the app's owner-scoping rule.
- **Reuse, don't fork:** MCP needs **data**, but every `decks.rs` `*_core` returns an HTTP `Response`. Factor **data-returning** helpers (e.g. `list_decks_data`, `fetch_deck_data`, `create_deck_data`, `update_deck_data`) that the existing HTTP handlers then wrap into responses, so `update` keeps using the **same BEGIN IMMEDIATE revision-snapshot transaction** — the snapshot policy must not be duplicated or bypassed.

### Frontend
- **`/app/tokens`** (any signed-in user): create a token by name → the value is revealed **once** in a copyable box with a clear "you won't see this again" warning; list existing tokens (name, created, last used, status) with revoke; plus short **connection instructions** showing the endpoint URL (`<origin>/mcp`) and a ready-to-paste MCP client config snippet. Nav entry alongside Fonts.
- All copy in `messages.ts` (TH + EN); theme variables only (dark-mode clean).

### Out of scope
- MCP `resources`/`prompts` primitives, SSE/streaming responses, OAuth for MCP, deck deletion via MCP (destructive over a long-lived token — deliberately omitted), token scopes/expiry (revocation only).
- ~~stdio MCP shim~~ — moved **into** scope by the folded review below: Claude Desktop cannot send a static bearer header, so `tools/mcp-stdio-bridge.mjs` ships as part of this brief.

## Business rules
- A token grants exactly its owner's deck access — **never** admin functions, settings, the AI endpoint, other users' decks, or token management itself.
- The plaintext token appears in exactly one response, ever; it is never logged and never returned by any list endpoint.
- Revocation is immediate (checked per request).
- **MCP is inbound only — it adds NO outbound call**, so CLAUDE.md §2 is unchanged and viewer pages still make zero external requests.

## Deliverables
Migration 0009 · `tokens.rs` (owner CRUD + `McpUser` extractor) · `mcp.rs` (JSON-RPC + tools) · data-returning helpers factored out of `decks.rs` · router wiring (`/mcp` outside `/api`) · backend tests (token create/list/revoke owner-scoping + cross-user revoke blocked; plaintext returned once and never again; MCP 401 without/with a bad or revoked token; `initialize`/`tools/list`/`tools/call` happy paths; a token cannot read another user's deck; `update_deck` snapshots a revision; notifications get no body; migration roundtrip) · `/app/tokens` page + TH/EN copy.

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): `docker compose -p deckoala up -d --build` healthy + `/api/health`; `cargo test` (visible pass count, exit 0) + `cargo fmt --check` + `cargo clippy -- -D warnings`; `npm run check` (0/0) + vitest + build; `pr-review` PASS before commit.
- Runtime: mint a token in the UI (value shown once); with `curl -H "Authorization: Bearer …"` drive a real MCP handshake — `initialize`, `tools/list`, then `tools/call` `create_deck` and `update_deck` — and confirm the deck appears/updates in the web UI. Confirm **401** with no token, a garbage token, and a **revoked** token; confirm a token for user A **cannot** read user B's deck; confirm `update_deck` created a revision. Viewer pages still show **zero external requests**.
- Adversarial review (2 lenses — implementability + security/token-scoping) before AND after coding, findings folded, as with prior briefs.

## Folded review findings (pre-code adversarial review, 2026-07-21 — 48 agents)

### ⚠️ Client-compatibility constraint (surfaced to the user)
**Claude Desktop cannot supply a static bearer header** for a remote MCP server — its remote connectors negotiate OAuth. A static-token HTTP endpoint therefore works with **Claude Code** (`claude mcp add --transport http … --header "Authorization: Bearer …"`), with custom/self-built clients, and with any client that allows custom headers — **but not Claude Desktop directly**. **User decision ("เลือกเลย" — my call): ship HTTP + Bearer AND the small stdio shim.** OAuth is overkill for a self-hosted instance, but the shim is ~50 lines of dependency-free Node (stdin/stdout JSON-RPC ⇄ `POST /mcp` with the header) and it unlocks Claude Desktop, so the "connect AI from outside" goal is actually met. Ship it as `tools/mcp-stdio-bridge.mjs` with a documented Claude Desktop config snippet. OAuth stays deferred. The README/tokens page must state which clients work which way rather than implying universal compatibility.

### Auth / transport mechanics
1. **Do NOT emit a bare `WWW-Authenticate: Bearer`** — it pushes MCP clients into OAuth discovery, and those `/.well-known/*` probes currently fall through to the SPA, which answers **HTML 200** and confuses the client. Return a plain 401 JSON body, and make `/.well-known/*` return 404 rather than the SPA shell.
2. **`/mcp` needs its own state-carrying router** (`Router::new().route(...).with_state(...)` merged like `assets_router`/`fonts_router`) — not a bare line on the final builder chain.
3. **`/mcp` must carry its own `DefaultBodyLimit::max(4 MB)`.** The 4 MB layer is on the `/api` router only (`lib.rs:397`, added because "JSON escaping can double a 1 MB markdown payload"); outside it axum's default is 2 MiB, so `/mcp` would be stricter than the equivalent HTTP path.
4. **Add NO CORS layer.** A browser cannot set `Authorization` cross-origin without CORS, so none is needed; adding a permissive one at the root would weaken `/api`'s existing protection.
5. **JSON-RPC specifics to pin down:** id may be string|number|null; a message with **no `id`** is a notification → HTTP 202, empty body, never a JSON-RPC reply; decide and document batch handling (arrays) rather than leaving it implicit; parse errors → `-32700`, unknown method → `-32601`, bad params → `-32602`.

### The decks.rs data refactor (the biggest implementation risk)
6. **Validation must move INTO the data helper, not stay in the HTTP wrapper.** All bounds (`markdown_too_large`, `parse_title` empty/200-char/control-char, nothing-to-update) currently live in the `Response`-returning layer; the mechanical split would leave MCP create/update **unbounded**, letting a token write rows no HTTP path could create (and copying them into `revisions` on every snapshot). Give the helper a typed error (`enum DeckError { NotFound, TitleEmpty, TitleTooLong, TitleControlChars, MarkdownTooLarge, Db }`); the HTTP handler maps it to `json_error(...)`, the MCP dispatcher to `{ isError: true }`.
7. **`UpdateDeck`/`CreateDeck` fields are private** and so unconstructible from `mcp.rs` — the data helpers must take explicit typed parameters rather than those structs.
8. **`private_interfaces` will hard-fail `clippy -D warnings`** if a `pub(crate)` helper returns a module-private row type — keep visibilities consistent (as done in BRIEF-0008).
9. **`finish_deck_tx` both commits and formats a `Response`** and is shared with revision restore — splitting must not change the owner path's behaviour.
10. **Do NOT copy `shares.rs`'s owner resolution.** It deliberately looks up the deck's *real* owner (a share token authorises one deck regardless of owner); MCP must scope to the **token's user** instead. Copying that pattern would break owner scoping.

### Abuse / lifecycle
11. **Bound MCP writes** — they are the only automated write path with no throttle; apply the same markdown/title caps and add a modest per-token rate limit.
12. **`update_deck` is destructive even without a delete tool** — omitting delete does not make MCP safe: it drops the editor's `baseUpdatedAt` clobber guard, so a token can overwrite concurrent edits. Accept `baseUpdatedAt` (optional) and rely on the revision snapshot, and say so explicitly.
13. **Validate the token `name`** (non-empty, length-capped, control-char free) and **cap tokens per user** so the table can't be grown without bound. Tokens have **no expiry** — revocation only; state that as an accepted limitation.

### Testing
14. **The integration harness cannot send an `Authorization` header** (`common::send`/`send_raw` take only a cookie) — extend it before writing MCP tests, or the suite cannot exercise the endpoint at all.
15. Migration `0009_api_tokens` was authored alongside this brief; the "latest is 0008" reference above predates it.
