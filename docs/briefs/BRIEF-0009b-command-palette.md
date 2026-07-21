# BRIEF-0009b — Command palette + keyboard shortcuts

- **Status:** ready to build
- **Depends on:** BRIEF-0009 (i18n + dark mode — every string here goes through `t()`, every colour through a theme var)
- **Traces to:** `REQ-ANALYSIS-v1` §7 nice-to-have "คีย์ลัดครบชุด + command palette" (user approved all §7 items; sequenced one brief at a time), deferred out of BRIEF-0009 §Out-of-scope: "Command palette (⌘/Ctrl-K) + fuller keyboard shortcuts — own brief."
- **Scope shape:** **frontend only.** No migration, no new endpoint, no backend change, no new dependency.

## Goal
A power user drives Deckoala without reaching for the mouse: one key opens a palette that can run any app action
**and jump to any deck by title**, and the handful of actions worth a dedicated key have one.

## Scope

### Command registry (`$lib/commands.ts` — pure TS, no runes, vitest-importable)
- A `Command` is `{ id, section, labelKey, shortcut?, run(ctx) }`. Labels are **message keys**, never literal strings,
  so the palette re-labels itself when the language toggles (BRIEF-0009 pattern: `messages.ts` stays a pure catalog).
- `commands.ts` must import **nothing** from `$app/*` and nothing reactive: the existing tests (`messages.test.ts`)
  import by **relative path**, and a `$app/navigation` import would break them. Navigation, the deck list and
  page-local actions all arrive through the injected `ctx` — which is exactly what makes the registry testable.
- A command the user cannot perform is **absent**, not disabled-and-visible: the dashboard has no "save deck", a
  non-admin has no "admin settings".
- **Pages contribute their own commands.** `/app/+layout.svelte` publishes `registerCommands()` through Svelte
  context; the dashboard and the editor register their real handlers (`newDeck`, `importFile`, `saveNow`, …) in an
  `$effect` and deregister on cleanup. The layout must never reconstruct a page's local state — the dashboard's
  hidden file input and the editor's `saveNow` are not reachable from it.
- **No destructive commands.** Delete stays a dashboard button behind its `confirm()`. A palette is a place where a
  stray `Enter` runs the highlighted row; that must never be able to destroy a deck. (Rename/duplicate/new/import
  are all recoverable and stay.)
- **Deck results** are a separate, section-labelled source. `routes/app/+layout.ts` returns only `user` (verified),
  so there is no deck list to reuse: the palette **fetches `api.decks.list()` on first open** — not on every `/app`
  page load, which would tax `/app/fonts`, `/app/tokens` and `/app/admin` for nothing — and caches it.
  - Rows are **two lines**: the title, plus dimmed `t('dash.updated', { when: formatDate(updatedAt) })`. `New deck`
    creates decks titled "Untitled deck" (`decks.rs` `DEFAULT_TITLE`), so without the second line a real user's
    palette is a wall of identical rows. Deck rows sort `updatedAt` **descending**; the secondary line is **not**
    scored.
  - The cache is **invalidated after any deck mutation** (create / import / duplicate / rename / delete) and after a
    jump resolves — "refresh in the background" alone goes stale by design.
  - A **401/403** from that fetch follows the app-wide convention and sends the user to `/login`; only network
    errors and 5xx degrade to "app commands only" with an inline notice (`palette.decksUnavailable`).
- `score(query, text)` = case-insensitive **subsequence** match returning `null` for no-match and a rank otherwise
  (contiguous runs and word-start hits rank higher). Hand-rolled — a fuzzy-search dependency is not warranted — and
  it iterates **code points**, never `charCodeAt` ASCII assumptions, because it must match **Thai** labels.

### Palette (`$lib/components/CommandPalette.svelte`)
- Opens on **`Mod-K`** (`⌘` on macOS, `Ctrl` elsewhere) and **`Mod-Shift-P`**, anywhere under `/app/*`; also from a
  **header button**, because a keyboard-only entry point is unusable on the phones this app must support. On narrow
  viewports that button is **icon-only** so the header stays one row — `DeckEditor`'s `article` hard-codes
  `calc(100dvh - 8.5rem)`, so a wrapping header would push the editor into a page-level scrollbar.
- Filter input focused on open; `↑`/`↓` move, `Home`/`End` jump, `Enter` runs, `Esc` closes. The active row is
  scrolled into view.
- **Selection is tracked by command `id`, not index.** When the list changes without a keystroke (the deck fetch
  resolving, a background refresh) the selection is **re-resolved to the same id**, and a list update that would
  move the active row is **buffered until the next keystroke or close**. `Enter` is inert until the first
  `decks.list()` settles, with a visible pending row rather than a silently short list. Without this, a fetch
  landing between keystroke and `Enter` runs a command the user never highlighted.
- `run(ctx)` is invoked **synchronously** inside the `Enter`/click handler, *before* any `await`; closing and focus
  restore happen after it returns. Commands gated on transient user activation — `Import .md`
  (`fileInput.click()`), anything using `navigator.clipboard` or `window.open` — are silently dead otherwise,
  because an intervening `await tick()` or `await goto()` consumes the activation.
- A failed navigation surfaces an inline notice rather than failing silently. A jump to a deck deleted in another
  tab lands on `routes/app/+error.svelte` — **the app shell, header and palette all survive** — and the deck cache is
  dropped so the dead row is gone next time the palette opens.
  - *Deviation from the first draft, taken deliberately:* the draft also asked to pre-flight the id and keep the
    palette open with a pending indicator. Pre-flighting costs an extra round trip on **every** jump to guard a rare
    cross-tab race, and the `/app` error boundary already prevents the damage that mattered (losing the shell). Not
    implemented; revisit if users actually hit it.
- **Empty query shows a useful default list** (recent decks + common actions), not a blank box. No match → a
  "nothing found" row, never an empty dialog.
- Closing **restores focus** to whatever was focused before it opened (including the CodeMirror editor, via
  `view.focus()`).
- a11y: `role="dialog"` + `aria-modal="true"`; the input is a `combobox` with `aria-expanded`, `aria-controls` and
  `aria-activedescendant` pointing at the active `role="option"`; the list is a `role="listbox"`. Focus is
  **trapped** while open. The backdrop must satisfy svelte-check's a11y rules (this project requires **0 warnings**)
  — use a `<button>`-based backdrop or pair the click handler with the required keyboard handler and role.
- **Mobile:** top-anchored sheet, never vertically centred — a centred dialog that auto-focuses a text input sits
  behind the virtual keyboard. `top: max(0.75rem, env(safe-area-inset-top))`,
  `max-height: min(70svh, calc(100svh - 1.5rem))`, the list as the only scroller, and `overflow: hidden` on `<body>`
  while open (restored on close).

### Shortcuts (`$lib/shortcuts.ts` — pure matcher + the one shared table, vitest-testable)
Exports `SHORTCUTS: { group, keys, actionKey, scope }[]` — the **single source** for both the bindings and the help
sheet — and `match(event, platform)` → action id or `null`.

| Keys | Where | Action |
|---|---|---|
| `Mod-K`, `Mod-Shift-P` | `/app/*` | open the palette |
| `Mod-S` | editor | **save now** (`preventDefault`, so the browser's Save-page dialog never appears) |
| `?` | `/app/*` | open the shortcut help sheet |
| `n` | dashboard | new deck |
| `Esc` | any overlay | close it |

- **Typing must win.** Unmodified keys (`?`, `n`) are ignored when the target is an `input`, `textarea`, `select`,
  anything `contenteditable`, or inside CodeMirror. Modified keys (`Mod-K`, `Mod-S`) work everywhere, mid-edit
  included.
- `Mod-K`/`Mod-S` are registered **inside CodeMirror** via `Prec.highest(keymap.of([...]))`, not a `window`
  listener that CodeMirror's own keymap would beat. Verified against the installed CodeMirror: plain `Mod-k` and
  `Mod-s` are unbound, but **`Shift-Mod-k` is `deleteLine`** — nothing here may bind `Mod-Shift-K`.
- The editor is shared with anonymous `/s/{token}` pages, which have **no palette**: `Mod-S` (save) is registered
  there too — it is just the existing autosave on demand — but `Mod-K` is not.
- `Mod` resolves per platform, and the help sheet renders the **platform-correct glyph** (`⌘K` vs `Ctrl+K`); a
  Windows user must never be told to press `⌘`.
- Shortcuts are inert while an overlay is open, except `Esc`.

### Shortcut help (`$lib/components/ShortcutHelp.svelte`)
- `?` (and a palette command) opens a sheet rendering **every group in `SHORTCUTS`** — `/app`, editor **and present
  mode**, whose keys already exist in `present/[id]/+page.svelte` (`←/→/Space/PageUp/PageDown/Home/End/n/p/f/Esc`)
  and are the app's most keyboard-driven surface. Present mode does not host the sheet, so the sheet states the
  scope of each group. Same dialog/a11y/focus-restore rules as the palette.

### Wiring
- `/app/+layout.svelte` owns both overlays, the global key listener and the command registry context.
- **`routes/app/+error.svelte`** is added, so an error under `/app` still renders inside the app shell with the
  palette alive — today every `/app` error falls back to `routes/+error.svelte` at the root, which unmounts the
  shell, the header and the global key listener.
- All copy in `messages.ts` (**TH + EN**, both catalogs), theme variables only (light + dark clean).

### Out of scope
- Chorded sequences (`g d` style), user-remappable keys, a palette on anonymous `/s/{token}` pages or in `/present`,
  command history/recency ranking, fuzzy search over deck *content*, any backend search endpoint, and **deck
  deletion from the palette** (deliberately omitted — see above).

## Business rules
- The palette never exposes an action the current user cannot perform (admin entries need `isAdmin`) — and hiding is
  **not** the security boundary: the server still enforces every one of these, as it did before this brief.
- Deck jump results come from the session's own `api.decks.list()`; no new endpoint, no way to see another user's decks.
- Nothing here changes deck data beyond what an existing button already did, and nothing destructive is reachable.
- **No new external request** on any page: CLAUDE.md §2 unchanged (still exactly two server-side outbound calls;
  viewer pages still zero).

## Deliverables
`$lib/commands.ts` (registry + scorer) · `$lib/shortcuts.ts` (`SHORTCUTS` table + matcher) ·
`CommandPalette.svelte` · `ShortcutHelp.svelte` · `/app/+layout.svelte` wiring (context registry, key listener,
header button) · `routes/app/+error.svelte` · dashboard + editor command contributions · editor `Prec.highest`
keymap · TH/EN copy · vitest for the scorer and matcher (Thai labels, platform differences, typing-wins, and
"list grows → selection stays on the same id").

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): `docker compose -p deckoala up -d --build` healthy + `/api/health`; backend suite
  still green (unchanged); `npm run check` **0/0** + vitest + build; `pr-review` PASS before commit.
- Runtime, in the browser:
  - `Mod-K` opens the palette **from the dashboard and from inside the CodeMirror editor**; typing a deck title and
    pressing `Enter` navigates there.
  - **Palette → `Import .md` actually opens the OS file picker** (proves the synchronous-`run` rule).
  - `Mod-S` in the editor saves and **no browser Save-page dialog appears**.
  - `?` opens the help sheet showing **`Ctrl`, not `⌘`**, on this Windows host, and it lists the present-mode group.
  - Typing `?` or `n` **inside** the editor or an input inserts the character instead of firing the shortcut.
  - `Esc` closes and focus returns where it was, including back into CodeMirror.
  - **Three decks named "Untitled deck" are tellable apart** in the palette.
  - 375px: the palette opens from the icon button, and with the keyboard raised the first and last rows are
    reachable; `/app/deck/[id]` at 375px **and 500px** has no page-level vertical scrollbar (header stays one row).
  - Light + dark both clean; the language toggle re-labels the palette; **zero external requests**.
- Adversarial review before AND after coding, findings folded, as with prior briefs.

## Folded review findings (pre-code adversarial review, 2026-07-22 — 318 agents, 8 lenses × 3 refute-by-default skeptics)
All eight lenses' findings were refuted on inspection. The completeness critic produced eight, all folded above:
1. **Async deck-list arrival re-ordered the list under the user's finger** — a fetch resolving between keystroke and
   `Enter` could run a command they never highlighted (the critic's scenario landed on `Delete deck`). → selection by
   id, buffered updates, `Enter` gated until the first fetch settles — and destructive commands removed entirely.
2. **Gesture-gated commands were silently dead** — `fileInput.click()` needs transient activation, which any `await`
   consumes. → `run(ctx)` runs synchronously before any `await`; pages contribute their own handlers.
3. **A jump to a stale deck id unmounted the whole app shell** — `/app/deck/[id]` raises `error(404)`, which resolves
   to the **root** `+error.svelte`, killing the layout that owns the palette and the key listener. → cache
   invalidation, inline 404 handling, and a new `routes/app/+error.svelte`.
4. **Every "New deck" is titled "Untitled deck"** → two-line rows with `updatedAt`, sorted newest first.
5. **A centred auto-focusing dialog sits behind the phone keyboard** → top-anchored sheet, `svh` sizing, body scroll lock.
6. **A header button would wrap the header** and break `DeckEditor`'s `calc(100dvh - 8.5rem)` → icon-only on narrow.
7. **"Degrade quietly on fetch failure" swallowed session expiry** → 401/403 goes to `/login`; only network/5xx degrade.
8. **The help sheet omitted present mode**, the most keyboard-driven surface → one shared `SHORTCUTS` table covering
   all scopes, with the scope stated per group.
