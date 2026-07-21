# Brief: Polish — Thai UI i18n + app dark mode

- **ID:** BRIEF-0009
- **Status:** ready
- **Created:** 2026-07-21 by Cowork (design)
- **Reference (read before coding):** `CLAUDE.md` (esp. §2 responsive-on-every-page + no-external-request invariant; brand `#F8F8FF`/`#0B1215`), `docs/design/ARCHITECTURE.md` §8 (BRIEF-0009 = Thai i18n + approved nice-to-haves), `docs/requirements/analyzed/REQ-ANALYSIS-v1.md` §7 ("Dark mode ของตัว UI (ไม่กระทบธีมสไลด์)"), `frontend/src/app.html`, `frontend/src/app.css`, and every page/component under `frontend/src/routes` + `frontend/src/lib/components` (all carry hardcoded English strings + surface colors to migrate)
- **Depends on:** BRIEF-0008 (done — commit `414fb3a`). **Frontend-only — no backend change.**
- **Language:** code/identifiers in English; UI copy now bilingual **Thai + English**
- **User decisions (this brief):** default UI language **Thai**, with a remembered toggle to English; include **UI dark mode**. (Command palette / keyboard shortcuts and theme gallery / per-deck CSS are approved but **sequenced into their own follow-up briefs**, not this one.)

## Goal
The user's "ใช้งานบนอุปกรณ์ทุกชนิดได้สะดวก" + REQ-ANALYSIS §7 polish: the whole UI reads in **Thai by default** (the user is Thai) with a one-click switch to English that is remembered, and the app chrome has a **light/dark toggle** (default from the OS preference, remembered) that never changes the **slide** theme — decks keep the `deckoala` look (`#F8F8FF`/`#0B1215`) in preview, present, and PDF regardless of the app's dark mode.

## Scope
### i18n
- **`$lib/i18n.svelte.ts`** — a rune-backed reactive `locale` (`'th' | 'en'`, default `'th'`), a `messages` dictionary `{ th, en }` keyed by dotted ids, and a `t(key, params?)` function reactive to `locale`. `setLocale()` persists to `localStorage['deckoala-locale']` and sets `document.documentElement.lang`. Init reads the stored value (else `'th'`). `t` on a missing key falls back to the `en` string, then the key itself (never throws, never blank).
- **`<LangToggle>`** control (TH / EN) in the app header + landing + login.
- Replace **every** user-facing hardcoded string with `t('…')`: landing, login (all 3 modes + errors), app layout (nav, user, log out), dashboard (toolbar, card actions, empty state, confirms, errors), editor (`DeckEditor` topbar, save statuses, revision banner, drop/upload hints, confirms), fonts page, `ShareManager`, `/s/[token]` (view + edit banners/buttons), `present` (audience + presenter labels, notices), `SharePresent`, the `+error` page. `<svelte:head><title>` strings too. Thai renders via the already-bundled Noto Sans Thai (no external request).

### Dark mode
- **`$lib/theme.svelte.ts`** — a rune-backed reactive `theme` (`'light' | 'dark'`, default from `prefers-color-scheme`, else `'light'`), `setTheme()` persists to `localStorage['deckoala-theme']` and sets `document.documentElement.dataset.theme`.
- **`app.css`** — promote the palette to semantic CSS variables (`--dk-bg`, `--dk-ink`, plus new `--dk-surface` [was `#fff`], `--dk-border`, `--dk-muted`, `--dk-danger`, `--dk-accent`, `--dk-accent-ink`); light values on `:root`, dark values on `:root[data-theme="dark"]`. Migrate hardcoded `#fff` / ink / `color-mix(... var(--dk-ink) ...)` surfaces across components to these variables so cards, inputs, borders, and the editor panes adapt. Keep contrast AA.
- **`<ThemeToggle>`** (☀/🌙) beside the language toggle.
- **The slide surfaces stay branded:** the Marp `deckoala` theme (preview/present/print) is NOT driven by the app variables — decks render `#F8F8FF`/`#0B1215` in both app modes. The present/audience stage stays dark (`#0B1215`) as today. Verify a dark-mode app still shows a light-branded slide.
- **`app.html`** — a tiny inline `<head>` script sets `data-theme` (and `lang`) from `localStorage`/`prefers-color-scheme` before first paint, so there is no light→dark flash; update `<meta name="theme-color">` to match.

### Out of scope (sequenced follow-ups, note in SESSION_STATE)
- Command palette (⌘/Ctrl-K) + fuller keyboard shortcuts — own brief.
- Theme gallery / per-deck custom CSS — own brief (touches slide rendering + a deck field).
- Server-side (per-user) persistence of locale/theme — localStorage is sufficient for now.
- Realtime collaboration, PWA/offline (deferred as larger later work).

## Business rules
- Default locale **Thai**; both locale and theme are **remembered** (localStorage) and applied before first paint (no flash).
- **No new external requests** — Thai font is already bundled; i18n/theme are pure client state. The no-external-CDN invariant holds.
- Every page stays **responsive** (desktop/tablet/mobile) and the toggles are reachable on small screens.
- Dark mode affects **app chrome only**, never the slide theme or exported PDF.
- `t()` must be total (fallback chain) — a missing key can never render blank or throw.

## Deliverables
`$lib/i18n.svelte.ts` (+ messages th/en) · `$lib/theme.svelte.ts` · `<LangToggle>` + `<ThemeToggle>` (or one `<Settings>` cluster) · `app.css` semantic variables (light+dark) · `app.html` pre-paint script · every page/component migrated to `t()` + theme variables · a small vitest for `t()` (fallback chain + a param substitution).

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): `docker compose -p deckoala up -d --build` healthy + `/api/health`; `npm run check` (svelte-check 0/0) + vitest + `npm run build`; `cargo test`/fmt/clippy unaffected (no backend change, but run to prove nothing broke); `pr-review` PASS before commit.
- Runtime, in the browser: default load is **Thai**; toggle → **English** flips all visible copy; reload **remembers** the choice. Toggle **dark** → app chrome (dashboard cards, editor panes, headers, inputs) goes dark with readable contrast, while the **slide preview/present render stays light-branded**; reload remembers it. Check a couple of pages at 375px. Network tab shows **zero external requests**. Thai glyphs render (bundled Noto Sans Thai).
- Adversarial review (2 lenses — implementability + i18n/theme coverage & consistency) before AND after coding, findings folded, as with prior briefs.

## Folded review findings (pre-code adversarial review, 2026-07-21 — 30 agents)
1. **Split the i18n module so vitest can test it.** `vitest.config.ts` has no Svelte plugin, so importing a `.svelte.ts` rune module throws. Put the message catalog + a **pure `translate(locale, key, params)`** in a plain **`$lib/messages.ts`** (no runes) and only the reactive `locale` + `t`/`formatDate` wrappers in `$lib/i18n.svelte.ts`. The vitest imports `messages.ts` (fallback chain + interpolation), never the rune module — no test-config change needed.
2. **Message values may be `string | (params) => string`** so a single mechanism covers interpolation, **pluralization** ("1 slide" / "2 slides"; Thai "N สไลด์"), and inline dates — the plain `t(key, params?)` contract wasn't enough for count-dependent copy.
3. **Locale-aware dates.** Timestamps use `toLocaleString()` with no locale, so they ignore the toggle. Add `formatDate(ts)`/`formatTime(ts)` in i18n that pass `th-TH`/`en-US` from the current `locale`; migrate every `new Date(...).toLocaleString()` call site.
4. **Browser-guard all globals.** `localStorage`/`matchMedia`/`document` must be read behind `import { browser } from '$app/environment'` (adapter-static may evaluate modules without a DOM; the node vitest has none). Init to the default (`th`/`prefers-color-scheme`) and reconcile from storage inside `browser`.
5. **`$state` export pattern.** `export let x = $state()` + reassignment is a compile error (`state_invalid_export`). Export a small object/class instance (`settings.locale`, `settings.theme` fields are `$state`) or getter+setter fns; never a reassigned top-level binding.
6. **Dark palette needs warn + success + AA'd danger.** Add `--dk-warn` / `--dk-warn-bg` / `--dk-warn-ink` and `--dk-success` (light+dark), and give `--dk-danger`/`--dk-success` distinct lighter hues on dark for AA. Migrate `DeckEditor` `.revision-banner` (`#fdf4dd`/`#b3862d` — and give it an **explicit** text color, not inherited `--dk-ink`) and `ShareManager` status colors (`#157347`/`#b3261e`) to these vars, so no light island survives in dark mode.
7. **Localize parent-injected props.** `DeckEditor`'s `backLabel`/`banner` come from its two call sites as static English — pass `t(...)` from the owner route and `/s/[token]`, and make the component's default `backLabel` a reactive `t()` fallback.
8. **Keep CSP `unsafe-inline`.** The pre-paint `<head>` script works under the existing CSP; do NOT add a nonce/hash (that would disable `unsafe-inline` and break SvelteKit's own inline bootstrap).
Known limitation (accepted, frontend-only scope): backend `ApiError` messages ("not found", validation text) stay English; only the app's own copy is localized.

## Folded post-code review (2026-07-21)
Confirmed findings folded: **CodeMirror had no dark theme** (high) — the editor gutter/caret/syntax stayed light on the dark surface; now a `Compartment` swaps a dark `EditorView.theme` + dark `HighlightStyle` in on the theme toggle. Low i18n/theme misses fixed: the `/s` view render-fail fallback, the slide-rail + present-region `aria-label`s (new keys), the present + landing `<title>`s, the Fonts `.hint code` chip tint (→ `color-mix`), and `savedAt` now stores the instant (formatted in the template) so it re-formats on a locale flip.
Accepted low limitation: a translated error/`logoutError`/status message already CAPTURED into `$state` (login/dashboard/editor/share) does not re-translate if the user toggles locale *while that transient error is on screen* — it self-corrects on the next action, and the `ApiError` branch is server-English regardless; a key-based re-translation of every transient error was judged not worth the cross-component churn for a self-correcting cosmetic case.
