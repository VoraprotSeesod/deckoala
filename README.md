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
| `CHROME_BIN` | `/usr/bin/chromium` (image) | Chromium binary for PDF export |

### Accounts

The first account registered on a fresh instance becomes the **admin**. Registration can be closed with `DECKOALA_ALLOW_SIGNUP=false`. Session cookies are HttpOnly/SameSite=Lax; on HTTPS set `DECKOALA_SECURE_COOKIE=true`. Mutating API requests from foreign origins are rejected — if your proxy rewrites `Host`, set `DECKOALA_PUBLIC_URL`.

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
