# Brief: Present mode + speaker notes + presenter view

- **ID:** BRIEF-0005
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (all), `docs/design/ARCHITECTURE.md` §3, §6.3 (present flow), `decisions/ADR-0002-slide-engine.md` (Marp client-side — present reuses the same render pipeline), `docs/briefs/BRIEF-0003-editor-preview.md` (marp.ts, shadow-DOM render, CSP), `frontend/src/lib/marp.ts`, `frontend/src/routes/app/deck/[id]/+page.svelte` (editor to add the Present entry point)
- **Depends on:** BRIEF-0004 (done — commit `8028a4a`)
- **Language:** code/identifiers/UI copy in English

## Goal
Deliver the user's "กด Present ผ่านเว็บ": a full-screen browser presentation of a deck with keyboard/touch navigation, plus a presenter view (current + next slide, speaker notes, elapsed timer) on a second window, the two kept in sync. Speaker notes come straight from Marp (HTML comments), never shown on the slide.

## Scope
### Backend
- **None.** Present reuses `GET /api/decks/{id}` (owner-scoped, from BRIEF-0002). Notes are parsed client-side from the Marp render. No migration, no new endpoint, no schema change. (Share-token access to present arrives with BRIEF-0008.)

### In scope — frontend
- **Extend `renderDeck` (`src/lib/marp.ts`)** to also return per-slide speaker notes: marp-core's `render()` already returns `comments: string[][]` (one array per slide of its non-directive HTML comments). Return `notes: string[]` = `comments.map((c) => c.join('\n\n'))`, keeping `slideCount = comments.length`. Notes are plain text rendered as text (never `{@html}`) — no new XSS surface.
- **Route `/present/[id]`** (outside `/app`, so the app-shell guard doesn't apply — it needs its own load):
  - `+page.ts` load: `await api.decks.get(id)`; `ApiError` 401 → `redirect(307,'/login')`, 404 → `error(404)` (mirrors the editor's deck load). SSR already off globally.
  - `+page.svelte` renders the deck **once** via `renderDeck` and collects the per-slide `svg[data-marpit-svg]` nodes (from the returned `html`). Each display surface is its own `attachShadow` root that adopts the SAME `marpSheet` constructable stylesheet (the exact technique proven in the editor's `renderRail()`), into which the relevant slide's `svg.outerHTML` is cloned — so audience mode has ONE stage host, and presenter mode has TWO hosts (current + next). **Letterbox by the slide's intrinsic aspect ratio, NOT a fixed 16:9** — a `<!-- size: 4:3 -->` deck yields a 960×720 viewBox; use contain-scaling (`svg { max-width:100%; max-height:100%; width:auto; height:auto }` centered in a flex ink backdrop) so any ratio self-fits without distortion.
  - **Empty deck (`slideCount === 0`, e.g. cleared markdown):** show an explicit "This deck has no slides — back to editor" state; hide the counter/nav; do not index into an empty SVG list. Index is clamped to `[0, max(0, N-1)]`.
  - **Two modes on the one route, chosen by the `?presenter` query flag:**
    - **Audience mode (default):** full-viewport slide; a slide counter (`n / N`); a "Fullscreen" button (Fullscreen API — requires the user gesture, so a button, not auto); an unobtrusive control bar that auto-hides after ~2 s of no pointer movement; "Open presenter view" and "Exit" (back to `/app/deck/{id}`) controls.
    - **Presenter mode (`?presenter`):** two-column layout — current slide (left) + next slide (right, cloned into a second shadow host, or "End of deck" when `index+1 >= N`); speaker notes for the current slide below (rendered as **text**, never `{@html}`); an elapsed timer (mm:ss) with pause/reset; slide position `n / N`; prev/next buttons. Desktop-oriented but must not break at 375 px (stack columns).
  - **Navigation:** keyboard `→`/`Space`/`PageDown`/`n` = next, `←`/`PageUp`/`p` = prev, `Home`/`End` = first/last, `f` = toggle fullscreen (audience only). **Ignore nav keys when `event.target` is a button/input/textarea/link** (so Space on a focused Pause button doesn't also advance), and blur controls after click. **`Esc`:** do NOT branch on `document.fullscreenElement` read at keydown (the UA already consumed the Esc to exit fullscreen and the read races). Track fullscreen via the `fullscreenchange` event; treat Esc as "leave" only when NOT fullscreen AND not within a short cooldown (~300 ms) after a fullscreen-exit — audience leaves to `/app/deck/{id}`, presenter closes its window.
  - **Cross-window sync (with an init handshake — BroadcastChannel has no retained value):** a `BroadcastChannel('deckoala-present-<id>')` shared by both windows. Messages: `{type:'nav', index}` on a user navigation (receiver adopts, does NOT rebroadcast → no loop); `{type:'hello'}` posted by a newly-mounted window; `{type:'state', index}` sent in reply to `hello`. A window adopts an incoming `state` only until it has synced once (`synced` flag), so an opener on slide 8 replies 8 and the fresh presenter adopts 8 — fixing the "presenter opens on slide 0 while audience is on 8" divergence. Belt-and-suspenders: "Open presenter view" also seeds the index via the URL hash (`window.open('/present/<id>?presenter#<index>', 'deckoala-presenter-<id>')`), read on mount. Channel closed on unmount.
- **Entry points:** add a **"Present"** action to the editor top bar (`/app/deck/[id]`) linking to `/present/[id]`, and a "Present" action on each dashboard deck card (`/app`). Opening present does not disturb editor autosave state (separate route/window).
- **Responsive:** audience mode fills any viewport (letterboxed); swipe works on touch; presenter columns stack under ~700 px. No horizontal page scroll at 375 px.
- CSP unchanged (same-origin assets/fonts already permitted; present renders the same marp output as the preview).

### Out of scope (later)
- Laser-pointer/pen/annotations, slide transitions/animations, remote-control from a phone as a separate device, auto-advance/rehearse timings, exporting the presenter notes; PDF export (BRIEF-0006); font manager (0007); share-token present access (0008)

## Data model
None (no persistence). Timer + current index are in-memory per window; sync is peer-to-peer via BroadcastChannel.

## Business rules (the important bits)
- Present is owner-scoped via the existing deck GET (foreign/soft-deleted → 404), same as everywhere.
- Notes are Marp HTML comments; they must appear ONLY in presenter mode, never on the rendered slide (Marp already keeps `<!-- … -->` out of the slide with `html:false`).
- The render pipeline is the shared `marp.ts` (WYSIWYG with editor/preview/print — ADR-0002). No second renderer.
- Rendered slide/notes CSS + markup follow the BRIEF-0003 XSS rules: marp CSS via adopted constructable stylesheet, notes as text content.

## API / interface surface
No backend surface change. New client routes `/present/[id]` and `/present/[id]?presenter`.

## Deliverables
- `renderDeck` notes addition, `/present/[id]/+page.ts` + `+page.svelte` (both modes), Present entry points on the editor and dashboard, any shared present helper, and the verification below.

## Verification / acceptance gate
- **Runs proof:** `docker compose -p deckoala up -d --build` succeeds; health unchanged. **UI flow in a real browser (desktop + 375 px):** from the editor, click Present → `/present/{id}` shows slide 1 full-viewport on the ink backdrop; `→`/`Space` advance and the counter updates; `f` / the button enters fullscreen; open presenter view → a second window shows the current slide + next-slide preview + the deck's speaker note + a running timer; advancing in the audience window moves the presenter window and vice-versa (BroadcastChannel sync); a deck containing `<!-- a note -->` shows that note in presenter view and NOT on the slide; `Esc`/Exit returns to the editor; on 375 px, swipe advances slides with no horizontal scroll.
- Tests executed: existing `cargo test` suite still green (Docker verify stage — no backend change, so no regression); `npm run check` exit 0; `npm run test:unit` (vitest) exit 0; `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` clean.
- Isolation unchanged (only port 8321, `deckoala-*`). Run the `pr-review` skill before committing.
