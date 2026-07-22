# BRIEF-0009c — Theme gallery + per-deck custom CSS (safe frontmatter writer)

- **Status:** ready to build
- **Depends on:** BRIEF-0003 (editor/preview/marp), BRIEF-0008 (share edit), BRIEF-0009 (i18n + dark mode), BRIEF-0009b (modal/overlay contract + palette registry)
- **Traces to:** `REQ-ANALYSIS-v1` §7 "Theme gallery + custom CSS ต่อเดค".
- **Split note:** the in-app slide guide (docs) + easier image insertion the user also asked for on 2026-07-22 moved to **BRIEF-0009d** — the pre-code review (253 agents) showed this cluster (frontmatter writer + themes + custom CSS + sanitizer) is security/correctness-critical and tightly coupled, and should not share a pass with the two independent, lower-risk features. 0009d ships right after this.
- **Scope shape:** mostly frontend; **one small, additive backend change** (sync the `theme` column from frontmatter on update). No migration.

## Goal
Pick a deck's look from a visual **theme gallery**, and tweak it with **per-deck custom CSS** — without ever corrupting the deck's Markdown or letting a shared page fetch anything external.

## Ground truth (verified before writing)
- `marp.ts` registers exactly one theme (`themeDeckoala`); `renderDeck()` renders whatever the markdown's `theme:` frontmatter directive names — there is no theme argument.
- The deck `theme` TEXT column (`decks.rs`) is set at create and **never updated**; it does not drive rendering and will actively mislead once the gallery ships.
- `slides.ts` already exposes `frontMatterEnd()` + `splitDeck()` built on **marpit's own tokens** — the authoritative frontmatter boundary. The writer MUST reuse it, never re-derive detection.
- Marp runs `html: false` (raw HTML escaped). Per-deck CSS is expressible via a Marp `style:` frontmatter block, scoped to the deck.
- **A strict CSP is already served** (`lib.rs`): `default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'`. External `url()`/`@import`/`![](https://…)` are therefore **already blocked at the browser** on every served page, including `/print` (PDF). The custom-CSS sanitizer below is **defense-in-depth**, not the sole guarantee.
- `renderDeck()` is called from **six** surfaces: DeckEditor preview, DeckEditor slide rail, `present/[id]`, `print/[id]`, `s/[token]` view, and `SharePresent.svelte` — plus the new gallery thumbnails will be a seventh. There is **no existing single choke point**.

## Build order (mandatory — each step verified before the next)
1. `frontmatter.ts` + exhaustive vitest, **in isolation, before any caller**.
2. `css-sanitize.ts` + vitest.
3. Bake the sanitizer **inside `renderDeck()`** so all six+ surfaces are covered by construction.
4. Themes + `marp.ts` registration.
5. Theme gallery modal.
6. Custom-CSS editor modal.
7. Backend `theme` column sync.

## Scope

### 1. Safe frontmatter writer — `$lib/frontmatter.ts` (pure, no `$app/*`; the single riskiest piece)
- **Detection reuses `slides.ts`** (`frontMatterEnd` / the marpit boundary). The writer never re-implements "is there frontmatter" — divergence from Marpit is how a deck gets corrupted.
- API: `getDirective(md, key)`, `setScalar(md, key, value)` (for `theme`), `setBlock(md, key, lines)` (for `style: |`), `getBlock(md, key)`. All operate on the **leading document frontmatter only**.
- Rules, **each unit-tested** (the body must be byte-identical except the edited directive):
  - **No frontmatter** → create a minimal fence (`marp: true` + the directive) ahead of the body. Body that *starts with a `---` horizontal rule* is not frontmatter (marpit boundary decides) → still create a fresh fence above it.
  - **Frontmatter-only** doc (no body) and **body-only** doc both handled.
  - **Replace the first ACTIVE key only.** A commented `# theme:` is not active. Never create a duplicate `theme:` (a duplicate key makes YAML fail and the deck renders nowhere); if the deck already has duplicates, collapse to one on write.
  - **`style: |` literal block:** the writer **owns indentation** — every user-CSS line is indented under the block, so a line that is `---`, or de-indented, or contains `:` cannot break out of the block or be read as a slide fence. Replacing an existing `style:` block replaces exactly its indented extent (determined by indentation), preserving trailing directives.
  - **CRLF preserved**, and lines the writer *inserts* match the document's existing newline style (no mixed endings).
  - Preserve all other directives, key order, and comments verbatim. Values with quotes or a trailing `# comment` are handled.
  - Round-trip stable: `set → parse → set` is a fixed point.
- **Do NOT hand-roll a YAML emitter for arbitrary data.** Only `theme` (scalar) and `style` (block) are written; everything else is opaque pass-through text. This bounds the blast radius.
- Add a `splitDeck` test proving a `style: |` block that contains a `---` line produces **no phantom slide** (segmentation already uses marpit tokens, so this should hold — the test guards it).

### 2. Custom-CSS sanitizer — `$lib/css-sanitize.ts` (pure, unit-tested; defense-in-depth over the CSP)
- Operates on the **user's custom-CSS string** (the content of the `style:` block) — **never** on `renderDeck()`'s output — so marp's theme CSS and KaTeX's `@font-face url(/katex-fonts/…)` are never touched (sanitizing the output would break math on every deck).
- Steps (order matters):
  1. **Strip CSS comments** first (`/* … */`), so split-token tricks (`@im/**/port`, `ur/**/l(...)`) collapse before matching.
  2. Remove every `@import` (at-rule) outright.
  3. For every `url( … )` occurrence — in any property/at-rule incl. `background`, `cursor`, `@font-face src`, `image-set()`/`-webkit-image-set()`, `src()`/`image()` — unquote, **decode CSS/unicode escapes** (`\74`, `\0074`, backslash-newline), trim, then **neutralize** it unless the target is `data:` or **same-origin** (a root-relative `/…` or a relative path). External schemes (`http:`, `https:`, `ftp:`, any `scheme:`) and protocol-relative `//host` are replaced with a harmless `url(about:blank)` (or the declaration dropped).
  4. Strip `expression(...)`, `javascript:`, `-moz-binding`, `behavior:` defensively.
- Returns cleaned CSS. **Robustness is unit-tested against the bypass list above** (comments, escapes, unquoted/quoted, mixed case, `//host`, image-set, cursor, font-face). Because the CSP already blocks the network fetch, the **unit test — not a network check — is the proof** (a network check would pass even with a no-op sanitizer).

### 3. Wire the sanitizer into the one render choke point
- Inside `renderDeck()` (`marp.ts`), before handing the markdown to marp: extract the `style:` block (via `frontmatter.ts`), run `css-sanitize`, and substitute the cleaned block back. Every current and future caller (all six surfaces + gallery thumbnails) is covered because they all go through `renderDeck()`. `renderDeck()` stays pure/node-importable (string ops only).

### 4. Themes
- Ship three curated, self-contained, **Thai-capable** (`'Inter','Noto Sans Thai',system-ui`), **zero-external** themes, all registered in `marp.ts`:
  - `deckoala` — existing light brand (unchanged).
  - `deckoala-dark` — ink bg `#0b1215`, off-white ink; dark-room talks.
  - `deckoala-bold` — light, oversized type + heavy headings; keynote impact.
- Each theme also defines a **`section.columns { column-count: 2; column-gap: 1.5em; }`** utility so real two-column layout works via a Marp **class directive** `<!-- _class: columns -->` under `html:false` (no raw HTML). (0009d documents it; the CSS ships here.)
- Not exposing marp-core `gaia`/`uncover` (their font stacks omit Noto Sans Thai → inconsistent Thai).

### 5. Theme gallery modal — `ThemeGallery.svelte`
- Opened from the editor top bar and the editor's palette command array. **Live thumbnail** per theme: the deck's current first slide rendered under that theme, in a shadow root via `adoptedStyleSheets` (reuse the rail pattern; never `{@html}` into light DOM). Thumbnails render a **frontmatter-swapped copy** of the markdown through `renderDeck()` — the live document is never mutated to preview.
- The active theme is marked; clicking one **applies** it: `setScalar(md,'theme',name)` → pushed through the editor's existing **`applyEditorContent()`** path (so undo history, autosave, and the revision snapshot all behave), then **the pending save is flushed** before the change counts as applied (so an immediately following PDF/present doesn't render the stale theme).
- Follows the BRIEF-0009b modal contract: `role="dialog"`/`aria-modal`, Esc, focus trap + restore, backdrop that passes svelte-check 0-warnings, body scroll-lock restore. The layout's `anyOverlayOpen()` already treats any open `role="dialog"` as an overlay, so `Mod-K`/`n` won't fire behind it.

### 6. Per-deck custom CSS — `CustomCssModal.svelte`
- A second CodeMirror (CSS language) in a modal, seeded from `getBlock(md,'style')`, persisted via `setBlock(md,'style',lines)` → `applyEditorContent()` → flush. Raw CSS is stored in `style:`; sanitization happens at render (step 3), so the editor shows exactly what the user typed.
- **The focus trap must not blanket-`preventDefault` Tab** (that breaks CodeMirror indentation) — trap by cycling within the modal's own tabbables, letting the CSS editor keep Tab. `Mod-K`/`Mod-S` inside this CodeMirror should not open the deck palette/save the deck (scope them out or let them no-op).

### 7. Backend: keep the `theme` column honest
- In `update_deck_data` (and `create_deck_data`), derive the theme from the **first active `theme:` frontmatter line** of the markdown and store it in the `theme` column, so the stored metadata never disagrees with what renders (the exact bug BRIEF-0011 flagged). Small, deterministic parse; owner path only; no new endpoint.

### Out of scope
- Slide guide / docs and image picker (→ **BRIEF-0009d**). User-uploaded theme files, a theme marketplace, instance-wide CSS, CSS beyond the per-deck `style:` block.

## Business rules
- Editing frontmatter (theme or custom CSS) never alters the slide body; the writer proves body-byte-invariance in tests.
- Viewer pages stay **zero external request** — guaranteed by the CSP and reinforced by the sanitizer inside `renderDeck()`; both apply to share/present/print.
- A share-**edit** guest can set theme + custom CSS; the sanitizer runs on the **owner's** subsequent view too (it's inside `renderDeck()`), so a poisoned deck can't exfiltrate from the owner either.
- Theme/CSS edits autosave and snapshot like any other markdown change; nothing bypasses the BEGIN IMMEDIATE revision transaction.
- All copy via `t()` in **both** catalogs; a new **catalog-parity vitest** asserts `th` and `en` have identical key sets (translate() falls back TH→EN silently, so a missing Thai key would otherwise ship as English, not a raw key). Theme vars only; light + dark clean incl. dark-mode control legibility (the `color-scheme` fix); responsive to 375px; every new modal verified in **dark mode**.

## Deliverables
`$lib/frontmatter.ts` (+ exhaustive tests) · `$lib/css-sanitize.ts` (+ bypass tests) · sanitizer wired inside `renderDeck()` · `theme-deckoala-dark.ts` + `theme-deckoala-bold.ts` + `.columns` utility + `marp.ts` registration · `ThemeGallery.svelte` (live thumbnails) · `CustomCssModal.svelte` · editor top-bar buttons + palette entries folded into DeckEditor's **single** `register([...])` array · backend `theme`-column sync (+ test) · TH/EN copy + catalog-parity test.

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): `docker compose -p deckoala up -d --build` healthy + `/api/health`; `cargo test` + `cargo fmt --check` + `cargo clippy -- -D warnings`; `npm run check` 0/0 + vitest + build; `pr-review` PASS before commit.
- **Unit (the real proof for security-critical pieces):** frontmatter writer round-trips and body-byte-invariance across every edge case (no-frontmatter, frontmatter-only, body-only, body-starts-with-`---`, CRLF, existing/commented/duplicate `theme:`, `style:` block with a `---` line); css-sanitize neutralizes every bypass in the list and **preserves** `data:`/`/assets/`/`/katex-fonts/` and same-origin refs.
- Runtime, in the browser:
  - Theme gallery shows **three live thumbnails**; pick `deckoala-dark` → preview + present + **PDF** all render dark; the frontmatter reads `theme: deckoala-dark` and the slide body is **byte-identical** otherwise; the `theme` column now reads `deckoala-dark`.
  - Add custom CSS recoloring `h1` → applies in preview and PDF; a custom-CSS `background:url(https://evil/x)` produces **no external request** (CSP + sanitizer) and the **unit test** confirms the sanitizer neutralized it; **math still renders** (KaTeX fonts untouched).
  - Both new modals: Esc closes, focus restores, `Mod-K`/`n` don't fire behind, and both are **legible in dark mode** (no dark-on-dark).
  - Every surface zero external requests; light + dark + 375px clean.
- Adversarial review (frontmatter correctness + sanitizer robustness + owner-scoping + i18n/UX) before AND after coding, findings folded, as with prior briefs.
