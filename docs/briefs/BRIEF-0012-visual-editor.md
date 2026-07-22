# BRIEF-0012 — Visual editor (phase 2): formatting toolbar + block inserter

- **Status:** ready to build
- **Depends on:** BRIEF-0003 (CodeMirror editor + preview), BRIEF-0009 (i18n + dark mode), BRIEF-0009b (palette/shortcut conventions), BRIEF-0009c (frontmatter writer + `bodyStartOffset` guard), BRIEF-0009d (image picker + `insertAtCursor`)
- **Traces to:** `REQ-ANALYSIS-v1` Q3 ("เฟสแรกแบบง่าย … แล้วค่อยต่อยอด visual editor ใน roadmap") and §7 ("Visual editor เต็มรูปแบบ, phase 2"). Phase 1 (slide reorder + drag files) shipped in BRIEF-0004; this is phase 2.
- **Scope shape:** **frontend only.** No migration, no endpoint, no backend change, no new dependency.

## Goal
Let a user who does not want to write Marp Markdown by hand **build and format slides by clicking** — bold/italic/headings/lists/links, and insert blocks (image, table, code, math, two-column, center, slide break, speaker note) — while the deck stays **standard Marp Markdown** (ADR-0002, no lock-in). The toolbar edits the same Markdown the CodeMirror editor shows; there is no separate document model.

## Ground truth (verified)
- The editor is CodeMirror 6 in `DeckEditor.svelte`; edits dispatch through `view.dispatch(...)`, and programmatic changes must run through the `applyingRemote` + `syncFromEditor()` path (autosave + re-render + revision snapshot). `insertAtCursor()` (BRIEF-0009d) already does this and clamps to `bodyStartOffset()` so nothing lands before the frontmatter.
- The image picker (`ImagePicker.svelte`), theme classes (`.center`/`.columns` from 0009c) and the guide already exist and must be reused, not duplicated.
- Marp runs `html: false` — every block this inserts must render without raw HTML (class directives, not `<div>`).
- The editor is shared by `/app/deck/[id]` (owner) and `/s/{token}` (share-edit); the toolbar must work on both.

## Scope

### 1. Markdown transform module — `$lib/md-format.ts` (pure, no runes/`$app`, fully unit-tested)
The heart of the feature: selection-aware string transforms so the toolbar is testable without a DOM. Each takes the full text + a selection `{from,to}` and returns `{ text, from, to }` (new text + new selection).
- **Offsets are plain JS-string (UTF-16) offsets — NOT code points.** CodeMirror positions are UTF-16 code-unit offsets, and `String.slice/length` are the same, so the transforms agree with the editor by construction. Do **not** re-index to code points (that would desync every offset on any astral character). Thai is BMP, so it is unaffected either way — but the tests must include a deck that contains an emoji/astral character to prove offset agreement.
- **Inline toggles** — `wrapInline(text, from, to, marker)` for bold `**`, italic `*`, inline code `` ` ``, strikethrough `~~`:
  - Empty selection → insert the paired markers with the caret between them.
  - **Whitespace-safe wrap:** wrap only the trimmed core of the selection; leading/trailing spaces stay *outside* the markers (`** word **` doesn't render).
  - **Toggle-off (unwrap)** fires when the markers are immediately **outside** the selection OR the selection itself begins+ends with them. Detection is **exact-length**: for `*` (italic), the char just beyond must be `*` **and not** part of a `**` (bold) run, and vice-versa — so toggling italic never eats one `*` off bold text and toggling bold never mis-detects italic. Inline code/strike are unambiguous.
- **Line-prefix toggles** — `toggleLinePrefix(text, from, to, kind)` for headings (`# `/`## `/`### `), bullet (`- `), numbered (`1. `), quote (`> `). The selection is first expanded to whole lines.
  - **Detection is by regex family, not literal string:** numbered matches `^\d+\. ` (so an existing `2. `/`3. ` is detected and can toggle off); heading matches `^#{1,6} `; bullet `^[-*+] `; quote `^> `. Blank lines are skipped.
  - **Toggle off** when every non-blank touched line already carries that family; otherwise **add**, first stripping any existing block prefix of *any* family from the line (so heading↔heading, bullet↔numbered, and quote are mutually exclusive — one block prefix per line, never stacked).
- **Link** — `insertLink(text, from, to)`: `[selection](url)` with the caret selecting `url`, or `[text](url)` (caret on `text`) when empty.
- **Block insert** — `insertBlock(text, pos, block)`: the caller passes `pos = max(caret, bodyStartOffset)`. The block is placed **on its own lines**: snap `pos` to the start of the next line (never split a paragraph mid-line), and guarantee exactly one blank line before and after (so a `---` slide break can never be read as a **setext H2** underline of the preceding text — the DECK-CORRUPTION case `slides.ts` warns about). Blocks: slide break (`---`), table skeleton, fenced code block, math block (`$$ … $$`), two-column, center, speaker note. The class-directive and math/code block **strings are shared constants** (one module) with the guide so the toolbar, guide and themes never diverge.
  - *Known limitation:* on a **frontmatter-less** imported deck with the caret at the very top, a `---` slide break inserted at offset 0 could read as a frontmatter open; every Deckoala-created deck has frontmatter (bodyStartOffset > 0), so the clamp prevents it in practice — documented, not silently ignored.
- Round-trip/idempotence and caret-position invariants are unit-tested, including: unwrap of italic-inside-bold and bold-containing-italic; numbered toggle-off of `2. ` lines; heading/bullet/quote mutual exclusivity across a multi-line selection; whitespace-trimmed wrap; block insert forcing blank lines; and an astral-character deck for offset agreement.

### 2. Formatting toolbar — `EditorToolbar.svelte`
- A compact button row **above the CodeMirror pane** in `DeckEditor`. Groups: text (Bold, Italic, Strikethrough, Inline code), structure (Heading ▾ H1/H2/H3, Bullet list, Numbered list, Quote, Link), insert (Image, Table, Code, Math, Two columns, Center, Slide break, Speaker note). The **Image** button opens the existing `ImagePicker`; nothing is re-implemented.
- Each button runs the matching transform through **one** `applyTransform(fn)` helper that:
  1. no-ops when `viewingRevision` is set or `view` is null (same guard as `insertAtCursor`);
  2. reads `view.state.selection.main`, **clamps both ends to `bodyStartOffset(doc)`** — the BLOCKER the review caught: the clamp must cover **every** transform (Bold/Heading/Link included), not only block insert, or clicking a button on a never-focused editor (selection `{0,0}`) prepends before the `---` fence and corrupts the frontmatter;
  3. runs the pure transform to get `{text, from, to}`;
  4. dispatches a single change through the `applyingRemote` guard, replacing the whole doc and **setting the returned selection**, then `syncFromEditor()` (so undo is one step per action, autosave + snapshot + preview all fire, and the caret lands where the transform put it);
  5. returns focus to the editor (`view.focus()`).
- Buttons are icon + accessible label (`aria-label` + `title`); the row is a single `role="toolbar"` with `aria-label`. **Headings are three explicit buttons (H1/H2/H3), not a dropdown**, to avoid menu a11y. Every button is **disabled** while viewing a revision (visibly, not just inert).
- **A toggle to show/hide the toolbar**, persisted through the existing settings singleton (`i18n.svelte.ts` pattern — no new storage). Default on.

### 3. Keyboard shortcuts (through CodeMirror's keymap, `Prec.highest` like `Mod-S`)
- `Mod-B` bold, `Mod-I` italic, added to the **existing `Prec.highest` keymap** so they beat both `basicSetup`'s `defaultKeymap` (which binds `Mod-i` → `selectParentSyntax`) and any browser default; both `preventDefault` and `return true`. Their `run()` calls the **same guarded `applyTransform`** — so they honor the `viewingRevision` lock and the `bodyStartOffset` clamp (a raw keymap dispatch would bypass both). They do not touch `shortcuts.ts` (editor-local, not global). No other new keys; `Mod-K`/`Mod-S` are unchanged.

### 4. i18n / theme / responsive
- All labels via `t()` in **both** catalogs with **static keys** (no dynamically-built key strings — so the catalog-parity test actually covers all ~15 buttons). Theme variables only; light + dark clean incl. control legibility. The toolbar is a fixed-height flex row and `.cm-host` becomes `flex: 1; min-height: 0` so the added row does **not** overflow the editor pane or add a page scrollbar (the `.cm-host { height: 100% }` assumption the review flagged); at 375px the row **scrolls horizontally within its own container**, never the page. Verify in **dark mode**.

### Out of scope
- A true WYSIWYG canvas / contenteditable surface (would fight the Markdown source-of-truth and ADR-0002); drag-to-position elements on the slide; tables editing UI beyond inserting a skeleton; real-time collaboration; PWA.

## Business rules
- The Markdown stays the single source of truth — every toolbar action is a text transform on it; there is no parallel model to desync.
- Transforms never corrupt the frontmatter (block inserts clamp to `bodyStartOffset`) and never bypass autosave/the revision snapshot (all go through `syncFromEditor`).
- Everything renders under `html:false` (class directives, not raw HTML).
- No new external request; no new dependency. Works identically on owner and share-edit routes.

## Deliverables
`$lib/md-format.ts` (+ exhaustive tests) · `EditorToolbar.svelte` · `applyTransform()` wiring + toolbar show/hide toggle in `DeckEditor.svelte` · `Prec.highest` `Mod-B`/`Mod-I` keymap · TH/EN copy · vitest for every transform (wrap/unwrap, line-prefix toggle, heading exclusivity, link, block insert with frontmatter clamp, Thai text).

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): `docker compose -p deckoala up -d --build` healthy + `/api/health`; backend suite unchanged (still green); `npm run check` 0/0 + vitest + build; `pr-review` PASS before commit.
- Runtime, in the browser:
  - Select a word, click **Bold** → it becomes `**word**` and the preview bolds it; click Bold again → unwraps. Same for Italic (and `Mod-B`/`Mod-I`).
  - Put the caret on a line, click **H2** → `## ` prefixes it; **Bullet**/**Numbered**/**Quote** toggle their prefixes across a multi-line selection.
  - **Link** wraps a selection as `[word](url)` with the caret in `url`.
  - Each **Insert** block (table, code, math, two columns, center, slide break, note) appears on its own lines and renders correctly (columns → two columns, center → centered, note → presenter-only).
  - **Image** opens the existing picker and inserts as before.
  - With the caret at document start (unfocused editor), inserting a block still lands **after** the frontmatter (theme preserved).
  - Toolbar hides/shows via the toggle and persists; works on a `/s/{token}` edit link; light + dark + 375px clean; **zero external requests**.
- Adversarial review (transform correctness + editor-integration + i18n/a11y) before AND after coding, findings folded, as with prior briefs.
