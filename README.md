<p align="center">
  <img src="assets/brand/logo.svg" alt="Deckoala logo" width="120" />
</p>

<h1 align="center">Deckoala</h1>

<p align="center"><strong>Markdown presentations, self-hosted.</strong></p>

Write Markdown (with LaTeX math), see your slides live as you type, present straight from the browser, export pixel-perfect PDFs, and share decks with a link — all from a single self-hosted container.

> **Status:** early scaffolding. The editor and deck features land in upcoming build phases — see `docs/design/ARCHITECTURE.md` §8 for the roadmap.

## Self-host quickstart

Requirements: Docker with the compose plugin.

```bash
git clone <this-repo> deckoala && cd deckoala
cp .env.example .env          # optional: change the published port
docker compose up -d --build
```

Then open `http://localhost:8321` (or your `DECKOALA_PORT`).

To serve it publicly (e.g. `deckoala.dimenshade.com`), point your reverse proxy — Caddy, Traefik, Nginx Proxy Manager — at that port. TLS terminates at your proxy.

All persistent state (SQLite database, uploads, fonts, exports) lives in the `deckoala-data` volume mounted at `/data`. Back up that volume and you have backed up everything.

### Health check

```bash
curl http://localhost:8321/api/health
# {"status":"ok","version":"0.1.0","db":"ok","chromium":true}
```

`chromium: true` confirms the bundled headless Chromium (used for PDF export) is present.

## Environment

| Variable | Default | Meaning |
|---|---|---|
| `DECKOALA_PORT` | `8321` | Host port published by compose (compose-level) |
| `DECKOALA_BIND` | `0.0.0.0:8080` | Address the server binds inside the container |
| `DECKOALA_DATA_DIR` | `/data` | Where all persistent state lives |
| `DECKOALA_STATIC_DIR` | `./static` | Built SPA location (set to `/app/static` in the image) |
| `DECKOALA_ALLOW_SIGNUP` | `true` | Allow registration. The first account (admin) can always be created |
| `DECKOALA_SECURE_COOKIE` | `false` | Set `true` on HTTPS deployments to add the `Secure` cookie flag |
| `DECKOALA_PUBLIC_URL` | — | Public URL (e.g. `https://deckoala.dimenshade.com`); set it when your reverse proxy rewrites the `Host` header, so cross-origin protection accepts your domain |
| `DECKOALA_ROOT_PASSWORD` | — (built-in `Admin123456@`) | Password for the `root` admin seeded on the **first** start with an empty database. Set it before first start on any reachable instance; otherwise change the default from Admin settings (the app warns until you do) |
| `CHROME_BIN` | `/usr/bin/chromium` (image) | Chromium binary for PDF export |

### Research library

Upload the papers your slides draw from under **Research** (PDF, or `.txt`/`.md`). The text is extracted **on the
server** — no outbound call — and, when you generate slides with AI, you pick which papers to source from so the deck
is built from your research rather than invented. Figures embedded in a PDF are pulled out too and can be dropped onto
a slide from the image picker's **From research** tab (or over MCP).

Limits: 10 MB per upload, 50 documents per user, and figures are extracted as **JPEG** only (up to 40 per document,
5 MB each, at least 64×64) — a scanned/image-only PDF has no text to read and is rejected with a message saying so.

### Writing slides

Decks are standard Marp Markdown. See the [slide authoring guide](docs/USER-GUIDE.md) —
also available in the app under **Guide** — for how to center text, lay out columns,
insert and size images, use a different font per slide, write math, and add speaker notes.

### Accounts

A fresh instance seeds a bootstrap admin **`root`** with the password `Admin123456@` (override with `DECKOALA_ROOT_PASSWORD` **before** the first start). The app shows a persistent warning until that default is changed — do it from **Admin settings → Change password**. Seeding only happens when the database is empty and never touches an existing account; if you delete all users, the next account registered becomes the admin instead. Registration can be closed with `DECKOALA_ALLOW_SIGNUP=false`. Session cookies are HttpOnly/SameSite=Lax; on HTTPS set `DECKOALA_SECURE_COOKIE=true`. Mutating API requests from foreign origins are rejected — if your proxy rewrites `Host`, set `DECKOALA_PUBLIC_URL`.

### AI slide generation

Off until an admin configures it in **Admin settings → AI**. Pick a provider (**Anthropic**, or anything
**OpenAI-compatible** — including a local Ollama / LM Studio base URL), set the model and an API key, then enable it.
The key is write-only: it is stored on the instance and never returned by the API. With AI on, any signed-in user gets
an **AI** button in the editor to generate slides from a prompt and append or replace the deck.

Only the **server** ever calls the provider, and only for a signed-in user's explicit request — the AI button is not
available on anonymous share links. Pages served to viewers (share links, present mode, PDF export) continue to make
**zero** external requests.

### MCP server (connect an external AI client)

Deckoala exposes an [MCP](https://modelcontextprotocol.io) server at **`POST /mcp`** (JSON-RPC 2.0) so an outside AI
client — Claude Desktop, Claude Code, or anything else that speaks MCP — can list, read, create and update **your**
decks. Mint a token under **API tokens** in the app; the value is shown once and stored only as a SHA-256 hash. A
token acts as the user who created it and is scoped to that user's decks; revoking it takes effect immediately.
There is deliberately no delete tool.

Deck tools: `list_decks`, `get_deck`, `create_deck`, `update_deck` (an update snapshots the previous content as a
revision, exactly like the editor does).

Research tools: `list_research` (your uploaded papers), `list_research_figures` (figures extracted from one of them),
and `attach_figure` — copies a figure into a deck and returns the Markdown to place it, so an AI client can
illustrate slides with the paper's own charts. All are scoped to the token's owner.

For an HTTP-capable client, point it at the endpoint with an `Authorization` header:

```bash
claude mcp add --transport http deckoala https://deckoala.example.com/mcp \
  --header "Authorization: Bearer dko_…"
```

For a client that only speaks stdio, use the bundled zero-dependency shim ([tools/mcp-stdio-bridge.mjs](tools/mcp-stdio-bridge.mjs), Node ≥ 18):

```json
{
  "mcpServers": {
    "deckoala": {
      "command": "node",
      "args": ["/path/to/deckoala/tools/mcp-stdio-bridge.mjs"],
      "env": {
        "DECKOALA_MCP_URL": "https://deckoala.example.com/mcp",
        "DECKOALA_MCP_TOKEN": "dko_…"
      }
    }
  }
}
```

The MCP endpoint is **inbound only** — it adds no outbound call of its own.

## Development (native)

Requirements: Node ≥ 20, Rust (stable ≥ 1.88).

```bash
# terminal 1 — backend on 8080
cd backend
DECKOALA_DATA_DIR=../data cargo run

# terminal 2 — frontend dev server (proxies /api to 8080)
cd frontend
npm install
npm run dev
```

Quality gates (all must pass — CI-equivalent):

```bash
cd frontend && npm run check
cd backend && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
# or run the same Rust gates inside Docker (no local toolchain needed):
docker build --target backend-verify --progress=plain .
```

## Stack

SvelteKit (static SPA) · Rust/Axum · SQLite · Marp Core (client-side slide rendering) · headless Chromium (PDF). Decisions and rationale: [`decisions/`](decisions/), architecture: [`docs/design/ARCHITECTURE.md`](docs/design/ARCHITECTURE.md).
