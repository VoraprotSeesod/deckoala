# CLAUDE.md — Operating guide for the implementation tool (Deckoala)

> The user drives with **"ลุย <TASK>"** (e.g. `ลุย BRIEF-0000`): read the referenced brief + this file in full,
> implement the whole brief end-to-end, run it in the project's standard runtime, and stop at the acceptance gate.

## 0. Workflow contract (locked rules — project-agnostic)

### Rule 1 — Two roles, serial only (never parallel)
- **Cowork** = design, briefs, audit. **Implementation tool** (Claude Code / Cursor) = code. They run **one at a time**.
- Cowork stays idle while the implementation tool is building, and vice-versa. Running both at once causes the two
  sessions to pick up overlapping or stale work.
- **No AI tool may suggest parallelism** ("run both", "open 2 tabs", "no file overlap so it's safe"). Only the user can
  authorize parallel work, explicitly, per session. Default is always serial.

### Rule 2 — The cycle
1. **Cowork prepares** a brief (or a small buffer of briefs) and stops.
2. **User hands off** with a short command: `ลุย <BRIEF-ID>` to the implementation tool.
3. **Implementation tool builds** end-to-end through the Definition-of-Done gate, then stops and reports.
4. **User returns** ("เสร็จแล้ว" / "done") and Cowork **audits**.

### Rule 3 — Audit answers with exactly ONE of two outcomes
When the user says the implementation is done, Cowork responds with **one** of:
- **PASS** — every acceptance/DoD item has evidence. Report the verdict with per-item evidence, then **immediately
  queue the next brief** and state the next `ลุย <task>`.
- **FAIL** — any item lacks evidence. Emit a **copy-paste-ready FIX prompt** for the user to send straight to the
  implementation tool, formatted as:
  ```
  ลุย <BRIEF-ID> FIX — <bullet list of failed items + concrete remediation per item>
  ```
Never offer a third option, a menu of choices, or "or you could…". The user drives; Cowork serves.

### Rule 4 — No manual tasks for the user
When something breaks or a plan changes, Cowork **replans and assigns the work to the implementation tool** via a
handoff prompt. Do not tell the user to run commands, restart servers, edit files, or apply migrations by hand.
- **Only exceptions:** personal credentials (API keys, 2FA), external account signups, and real-world tasks
  (legal, contracts, DNS records at the registrar). Document where these go; don't track them as workflow tasks.

### Rule 5 — "Tests green" ≠ "it works"
The DoD gate requires proof the change **runs in the project's standard runtime** (the Docker stack comes up and
the health check passes), and that tests were **executed** (exit 0, visible pass count) — not merely written. For UI,
actually load the page; a server returning 200 is not proof the UI rendered.

### Rule 6 — Resume Pointer on top of SESSION_STATE.md
Every session reads `SESSION_STATE.md` first. The top line is a single concrete "do this next" so no context is lost
between sessions or tools.

### Rule 7 — Don't invent; trace to a source
Requirements, fields, and rules trace to a document, a data sample, or an explicit user decision. Ambiguities become
open questions — never silent assumptions.

## 1. Task queue — what "ลุย <TASK>" maps to (build in this order)
| Command | Read this brief | What you build |
|---|---|---|
| `ลุย BRIEF-0000` | `docs/briefs/BRIEF-0000-infra-scaffolding.md` | Repo scaffolding: SvelteKit + Axum + Docker, landing page, health check |
| `ลุย BRIEF-0001` | `docs/briefs/BRIEF-0001-auth-users.md` | Auth & users: register/login/logout, sessions, guarded /app shell |
| `ลุย BRIEF-0002` | `docs/briefs/BRIEF-0002-deck-crud.md` | Deck CRUD + dashboard: list/create/rename/duplicate/soft-delete/import/export |
| `ลุย BRIEF-0003` | `docs/briefs/BRIEF-0003-editor-preview.md` | Editor (CodeMirror) + Marp live preview + deckoala theme + autosave + revisions |
| `ลุย BRIEF-0004` | `docs/briefs/BRIEF-0004-slide-rail-assets.md` | Slide thumbnail rail (drag reorder) + image upload (drop/paste) |

> If the user says just "ลุย" with no task, build the next unchecked brief. Full roadmap: `docs/design/ARCHITECTURE.md` §8.

## 2. Stack (locked — do not substitute; see decisions/ADR-0001..0003)
- **Backend:** Rust stable + Axum + tokio; sqlx (SQLite, WAL); argon2id; tower-sessions; chromiumoxide (PDF)
- **Frontend:** SvelteKit (Svelte 5 + TypeScript), `adapter-static` SPA; CodeMirror 6; `@marp-team/marp-core` (client-side slide rendering); KaTeX via marp-core
- **Auth / AuthZ:** cookie sessions, owner-scoped queries, share-link tokens (view/edit)
- **Storage:** SQLite + files, everything under the `/data` volume only
- **Deploy / runtime:** single container from one `compose.yml`; Chromium inside the image (`CHROME_BIN`)
- Cross-cutting invariants from day 1: owner-scoped multi-user, soft delete on decks, UTC RFC3339 timestamps, deck format = standard Marp Markdown (no lock-in), responsive UI (desktop/tablet/mobile) on every page, and: pages served to viewers make **no** external CDN/font requests — the only permitted outbound call is the server-side Google Fonts download inside the font-manager install flow (fonts are then served locally from `/data/fonts`)
- Brand: background `#F8F8FF`, ink `#0B1215`; logo at `assets/brand/logo.svg` (+ `logo-dark.svg`)

## 3. Runtime rules (Docker isolation — MANDATORY)
- Compose project name **deckoala**; container `deckoala-app`; network `deckoala-net`; volume `deckoala-data`.
- Host port only `${DECKOALA_PORT:-8321}` → 8080 (documented in `.env.example`). The port ban applies to **Docker-published ports**: the compose stack must publish nothing but `${DECKOALA_PORT:-8321}`. Native dev processes are exempt (`cargo run` on 8080, Vite on 5173).
- Pin base image tags in the Dockerfile; clean teardown (`docker compose -p deckoala down`) removes only this project's resources.
- Host machine is Windows 11 + Docker Desktop; keep commands cross-platform (no bash-only Makefile assumptions in docs).

## 4. Acceptance gate
- The change **runs** in the standard runtime: `docker compose -p deckoala up -d --build` succeeds + `/api/health` passes + isolation respected.
- Tests executed (exit 0, visible pass count): `cargo test` for backend, `npm run check` (svelte-check) — and `cargo fmt --check` + `cargo clippy -- -D warnings` clean.
- Per-brief acceptance is in each brief's Verification section.
- Run the `pr-review` skill before every commit and before merging to main.

## 5. When you finish
1. Pass the acceptance gate (+ `pr-review`).
2. Append a dated entry to `SESSION_STATE.md` progress log; update the Resume Pointer.
3. Stop and report — the user verifies/audits before the next "ลุย".

## 6. Reading priority for new sessions
1. `SESSION_STATE.md` (Resume Pointer) → 2. this file → 3. the current brief + referenced ARCHITECTURE/ADR sections.
