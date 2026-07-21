# SESSION_STATE — Deckoala

> Read this file FIRST at the start of every session. The Resume Pointer below is the single next action.

## ▶ Resume Pointer
Cowork: audit BRIEF-0003 result (user returns with "เสร็จแล้ว/done"), then write BRIEF-0004 (slide thumbnail rail + drag & drop reorder + asset upload).

---

## 1. Project snapshot
- **What:** Deckoala — self-hosted Markdown/LaTeX presentation web app (live preview, present in browser, PDF export, custom fonts, share links) at `deckoala.dimenshade.com`
- **Who/for:** the user + people they share the instance with (multi-user)
- **Replaces:** ad-hoc slide tools; must be trivially self-hostable (one compose.yml)
- **Language convention:** discussion/analysis docs in Thai; briefs, code, identifiers, UI copy in English (Thai UI i18n on roadmap)

## 2. Roles & workflow
- **Cowork** = design, briefs, audit. **Implementation tool** = code. **Serial only** (see CLAUDE.md §0).
- Loop: Cowork brief → user `ลุย <task>` → impl builds → user "done" → Cowork audit (PASS→next / FAIL→FIX prompt).

## 3. Architecture decisions (summary)
- [ADR-0001](decisions/ADR-0001-stack.md): SvelteKit static SPA + Rust/Axum + SQLite, single container `[USER]`
- [ADR-0002](decisions/ADR-0002-slide-engine.md): Marp Core rendered client-side; deck format = standard Marp Markdown
- [ADR-0003](decisions/ADR-0003-pdf-export.md): PDF via headless Chromium in-container, driven by chromiumoxide
- Full design: [docs/design/ARCHITECTURE.md](docs/design/ARCHITECTURE.md) · Requirements: [REQ-ANALYSIS-v1](docs/requirements/analyzed/REQ-ANALYSIS-v1.md)
- Brand: bg `#F8F8FF`, ink `#0B1215`; logo `assets/brand/logo.svg` (selection log: [docs/design/brand/logo-candidates.md](docs/design/brand/logo-candidates.md))

## 4. Briefs / task queue
| Command | Brief | Status |
|---|---|---|
| `ลุย BRIEF-0000` | infra & scaffolding | **done** (2026-07-21, pr-review PASS) |
| `ลุย BRIEF-0001` | auth & users | **done** (2026-07-21, pr-review PASS) |
| `ลุย BRIEF-0002` | deck CRUD + dashboard | **done** (2026-07-21, pr-review PASS) |
| `ลุย BRIEF-0003` | editor + live preview + autosave + revisions | **done** (2026-07-21, pr-review PASS) |
| BRIEF-0004…0010 | see ARCHITECTURE §8 roadmap | queued |

## 5. Progress log
### 2026-07-21
- Requirements captured from user brief (Thai) → REQ-ANALYSIS-v1; discussion round 1 closed Q1–Q4 (framework SvelteKit, backend Rust, drag & drop phase-1 simple, multi-user + share links).
- ADR-0001..0003 accepted; ARCHITECTURE.md written; CLAUDE.md + skeleton laid down.
- Logo designed (4 candidates → 3-judge panel → `ears-monogram` won); saved to `assets/brand/` + visual check passed at 16–256px.
- BRIEF-0000 written and registered; adversarial doc-review (3 lenses, 12 findings) applied: scoped port/font invariants, added `db` to health gate, volume chown, `DECKOALA_STATIC_DIR`, reserved prefixes `/api|/assets|/fonts`, roadmap +BRIEF-0010 visual editor, presenter view → BRIEF-0005, PWA/theme gallery moved back to pending-user-choice.
- **BRIEF-0000 implemented** (same session, user said `ลุย`): SvelteKit SPA + Axum server + SQLite migrations (reversible pair) + Docker (multi-stage, chromium, non-root, healthcheck) + compose. Evidence: `cargo test` 2 passed / clippy `-D warnings` / fmt clean (Docker verify stage); `svelte-check` 0 errors; compose up → container healthy; `/api/health` = `{"status":"ok","db":"ok","chromium":true}`; landing verified in browser desktop + 375px mobile; teardown leaves only the volume. Rust pinned 1.88.0 (deps' MSRV forced bump from 1.84; Cargo.lock committed, builds `--locked`).
- Note for next briefs: down-migrations exist from 0001 but executing them needs sqlx-cli — bring it in with BRIEF-0001.
- **BRIEF-0001 implemented** (same session): register/login/logout/me + /api/instance; argon2id (pinned m=19456,t=2,p=1, spawn_blocking); tower-sessions SqliteStore (Lax, 30d, cycle_id on login, hourly expired-session cleanup); origin-check CSRF (authority vs Host or DECKOALA_PUBLIC_URL, null→403); atomic first-user-admin INSERT; login: uniform 401 + dummy-hash timing + 128-byte cap; SPA: /login 3-mode form, guarded /app shell, +error page. Two adversarial review rounds (design 11 findings → brief tightened pre-code; implementation 5 findings → all fixed, incl. Vite changeOrigin:false and logout-error surfacing). Evidence: 19 tests passed (Docker verify), svelte-check 0 errors, compose healthy, curl flow 201→200→204→401/403, browser UI flow desktop + 375px mobile (measured no h-scroll; pane input broke mid-run → clicks dispatched via DOM, same app handlers). New env: DECKOALA_ALLOW_SIGNUP / DECKOALA_SECURE_COOKIE / DECKOALA_PUBLIC_URL.
- Deferred to hardening brief (recorded in BRIEF-0001): rate limiting/lockout, register-path username enumeration.
- **BRIEF-0002 implemented** (same session): migration 0003 decks (+ `foreign_keys(true)` now enforced); AuthUser extractor; CRUD list/create/get/PATCH/soft-delete/duplicate/export with owner-scoping-as-404 on reads AND writes; default Marp template; export Content-Disposition (remove-filter ASCII + RFC5987 filename*); fixed-width `now_rfc3339` (lexicographic ordering); DefaultBodyLimit 4MB; dashboard (grid, new/import/rename/duplicate/export/delete, empty state) + read-only deck page. Two review rounds again (brief: 9 findings pre-code; code: 5 nits — all fixed incl. surrogate-pair-safe filename truncation and 401→/login on cached-layout page loads). Evidence: 37 tests passed (9 unit + 11 auth + 14 decks + 3 health), svelte-check 0/0, compose healthy, curl CRUD flow verified, browser flow desktop+375px incl. import-via-DataTransfer (title from filename), teardown clean earlier this session.
- Incident note: a tool-call edit containing backslash-u unicode escapes got decoded into literal control bytes inside +page.svelte (ripgrep then treated the file as binary) - rewrote the file cleanly; lesson: avoid backslash-u escape sequences in tool-call payloads, prefer codePoint filters in JS.
- **BRIEF-0003 implemented** (same session): migration 0004 revisions; transactional PATCH snapshot (BEGIN IMMEDIATE via sqlx begin_with → auto-rollback on drop, 300s throttle, stale-baseUpdatedAt forces snapshot for the clobber case, retention cap 50 with rowid tie-break); restore/list/get routes (owner+deck scoped, cross-deck revId 404); CSP header (blocks external egress from decks); CodeMirror 6 + marp-core client-side preview in Shadow DOM via **constructable stylesheets** (CSS never HTML-parsed → style-directive XSS breakout impossible), deckoala theme (#F8F8FF/#0B1215 + Noto Sans Thai), KaTeX with locally-bundled fonts (katexFontPath=/katex-fonts/ + document-level katex.min.css import — zero external requests); autosave state machine (debounce, retry, epoch guards, beforeNavigate flush for SPA nav, in-flight-save awaited before restore); revisions panel with view/restore. Two review rounds (brief: 8 findings folded pre-code incl. BEGIN IMMEDIATE, stale-base snapshot, CSP, sizeBytes-as-blob, cross-deck test; code: 6 findings fixed incl. the blocker SPA-nav data loss and the </style> XSS breakout → constructable stylesheets). Evidence: 49 tests passed (Docker verify), svelte-check 0/0, compose healthy, browser: live Thai+KaTeX preview / autosave persists across reload / revision view+restore / CSP blocks external img / XSS probe did not fire / desktop split + mobile tabs.
- launch.json written (.claude/launch.json: compose 8321, vite 5173, cargo 8080) — none started per user (compose stack already up).
- Deviation log (BRIEF-0003): none — KaTeX stayed on the primary (local-font) path; MathJax fallback not needed.

## 6. Open questions / blockers
- Q5 fonts: instance-level (default, `[STD]`) — revisit at BRIEF-0007 if the user objects.
- Q6 speaker notes: yes via Marp comments (`[STD]`); presenter view = roadmap.
- Q7 UI language: English first, Thai i18n at BRIEF-0009 (`[STD]`) — user may veto.

## 7. Pending tasks for the implementation tool
- None — next step is design-side: Cowork writes BRIEF-0003 (editor + Marp live preview + autosave + revisions), then the user hands off with `ลุย BRIEF-0003`.
