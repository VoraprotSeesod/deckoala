# BRIEF-0009d — In-app slide guide (docs) + easier image insertion

- **Status:** ready to build (after BRIEF-0009c)
- **Depends on:** BRIEF-0004 (asset upload/serve), BRIEF-0008 (share edit + EditorAdapter), BRIEF-0009 (i18n), BRIEF-0009b (modal contract + palette), BRIEF-0009c (themes ship the `.columns` class the guide documents)
- **Traces to:** the user's 2026-07-22 request: *"อย่าลืมทำ Doc วิธีการใช้งานด้วยนะโดยเฉพาะพวก Syntax ในการทำสไลด์ แล้วก็การจัดหน้าต่าง ๆ แล้วก็การใส่รูปในสไลด์อยากให้มันดูง่ายกว่านี้"* — in-app docs (esp. slide syntax + layout) and **easier image insertion**.
- **Scope shape:** frontend + **two additive backend read endpoints** (list a deck's assets, owner + share-edit). No migration.

## Goal
Make authoring **self-explanatory and low-friction**: an in-app **slide guide** teaches the Markdown/layout/image syntax, and an **image button** lets you upload *or reuse* an image already in the deck instead of only drag-and-drop.

## Ground truth (verified)
- Image insertion today is `DeckEditor.uploadAndInsert()` — drop + paste only, inserts `![alt](url)`; no toolbar entry, no reuse. `altText()` sanitizes filenames.
- Assets have **upload + serve** endpoints only; **no list**. The `assets` table has `id, deck_id, filename, original_name, mime, size_bytes, created_at` (URL derives as `/assets/{deck_id}/{filename}`) but **no owner column** — ownership must be resolved via the parent deck.
- `EditorAdapter` (api.ts) has `uploadAsset` but **no `listAssets`**; it is the seam shared by `/app/deck/[id]` (owner) and `/s/{token}` (share).
- `duplicate()` copies markdown + theme but **not** asset rows/files — a duplicated deck's images render (owner still owns the original path) but `GET …/assets` for the copy is empty.
- `renderDeck()` runs `html:false`, so **two-column layout needs a Marp class directive**, not raw HTML — BRIEF-0009c ships `section.columns { column-count: 2 }` in every theme; the guide documents `<!-- _class: columns -->`.

## Scope

### A. In-app usage manual — a browsable guide **in the website** (user request 2026-07-22)
The user asked for a real **คู่มือการใช้งาน in the web app**, not just a tooltip — covering, by name, **centering text**, **inserting images**, and **a different font per slide**. So the guide has two entry points sharing one content module (`$lib/guide-content.ts` — pure data: section title key + prose key + literal code snippet):
- **Standalone manual — `/app/guide`** (a real route, reachable from the app nav + the command palette on any `/app` page, and the "Guide" button). Renders the whole manual with **Copy** on every snippet. This is the "in-website manual." No Insert here (no editor).
- **Editor quick-guide — `SlideGuide.svelte`** opened from the editor top bar + the editor palette command (owner route and `/s/{token}` edit). Same content, plus **Insert at cursor** on each snippet (through the editor's existing cursor-dispatch path).
- **Prose/labels go through `t()`; the Marp snippets are literal code** (identical in both languages — never keyed, so they can't render as a raw key). Sections:
  - Structure: `---` slide separators, headings, lists, bold/italic, links, `code`, blockquotes, tables.
  - **Centering / จัดคำให้อยู่กลาง:** `<!-- _class: center -->` (the class BRIEF-0009c ships) to center a slide's text; note it is per-slide (the `_` prefix scopes it to that slide).
  - **Layout / การจัดหน้า:** **two columns** via `<!-- _class: columns -->`; background images `![bg](url)`, `![bg left](url)`, `![bg fit](url)`; padded vs full-bleed.
  - **Images / ใส่รูป:** `![alt](url)`, sizing `![w:400](url)` / `![w:400 h:300](url)`, backgrounds — the **exact syntax the image picker emits**, so guide and button agree; and a pointer to the 🖼 button.
  - **Per-slide font / เลือกฟอนต์แต่ละหน้าไม่ซ้ำกัน:** the end-to-end recipe — (1) install the font on the **Fonts** page (BRIEF-0007), (2) define a class in **Custom CSS** (BRIEF-0009c), e.g. `section.thai { font-family: 'Sarabun'; }`, (3) apply it to one slide with `<!-- _class: thai -->`. Show the three steps together; this is the feature the user explicitly asked for.
  - Math `$inline$` / `$$block$$`; speaker notes `<!-- note -->` (presenter only).
  - Theme + custom CSS: pointer to the gallery / custom-CSS modal (0009c).
- Verify each documented syntax **actually renders under this marp config** (`html:false`, `inlineSVG`, katex) before shipping it — especially the `center`/`columns` classes, per-slide `_class` font scoping, and image sizing.
- Also add a concise repo `docs/USER-GUIDE.md` mirroring the manual, linked from the README.

### B. Easier image insertion
- **Backend (additive):**
  - `GET /api/decks/{id}/assets` → `[{ id, url, originalName, mime, sizeBytes, createdAt }]`, **owner-scoped via the parent deck** (reuse the `*_data`/`DeckError` owner-resolution — the assets table has no owner column, so join/scope through `decks`). Foreign/deleted deck → 404. A `GET` on this path must coexist with the existing `POST` (method-routed, same path — fine in axum).
  - `GET /api/s/{token}/assets` → same shape, requires **edit** permission (mirror how `uploadAsset` is gated on the token), resolving the deck via the share.
- **`EditorAdapter.listAssets()`** added to the seam, with owner + share implementations, so the picker works identically on both routes.
- **Image picker — `ImagePicker.svelte`** (editor top-bar **🖼 Image** button + editor palette command; drop/paste still work):
  - **Upload** a new image (click/drop) → existing `adapter.uploadAsset` → insert.
  - **Reuse** an image already in the deck → thumbnail grid from `adapter.listAssets()`; click to insert. (A **duplicated** deck lists none — documented limitation; the grid shows an empty-state hint to upload.)
  - **Two mutually-exclusive modes**, because Marp's inline sizing and background syntax don't combine:
    - *Inline* → Small / Medium / Full = `![alt](url)` with `![w:320]` / `![w:640]` / no width.
    - *Background* → `![bg](url)` / `![bg fit]` / `![bg left]` (no `w:`).
  - Alt text defaults to the sanitized filename and is editable; alt text and Marp keywords never share a slot (keywords live in the directive position, alt in the `![…]` label).
  - Thumbnails have alt text and are keyboard-selectable; the grid reserves space to avoid load jank.
  - Insertion uses the existing shadow-DOM/CRLF-safe cursor dispatch; works on `/s/{token}` edit via the adapter.

### Out of scope
- Cross-deck asset library, asset deletion UI, copying assets on duplicate, image editing/cropping, rich WYSIWYG layout (→ visual editor, BRIEF-0012).

## Business rules
- The asset list is owner/permission-scoped exactly like deck reads; a token for deck A never lists deck B's assets; a **view** token cannot list (edit only).
- Nothing changes deck data beyond inserting Markdown the user chose; insertion autosaves/snapshots like any edit.
- Zero external requests preserved (CSP backstop; the picker only ever inserts same-origin `/assets/...`). All copy via `t()` both catalogs (+ the catalog-parity test from 0009c); theme vars only; every modal follows the 0009b contract and is **dark-mode legible**; responsive to 375px.

## Deliverables
`GET /api/decks/{id}/assets` + `GET /api/s/{token}/assets` (owner/edit-scoped, + backend tests) · `EditorAdapter.listAssets()` (owner + share) · `$lib/guide-content.ts` (shared manual data) · `/app/guide` route (browsable manual, Copy) · `SlideGuide.svelte` (editor modal, Copy + Insert) · nav + palette entry for the manual · `docs/USER-GUIDE.md` + README link · `ImagePicker.svelte` + editor top-bar buttons + palette entries folded into DeckEditor's single `register([...])` array · TH/EN copy · vitest for the size/mode→syntax mapping and the endpoint owner-scoping.

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): stack healthy + `/api/health`; `cargo test` + fmt + clippy clean; `npm run check` 0/0 + vitest + build; `pr-review` PASS.
- Runtime, in the browser:
  - Open the slide guide; **Copy** and **Insert** a two-column snippet and an image-sizing snippet → both render as documented (`<!-- _class: columns -->` shows two columns; `![w:320]` shrinks the image).
  - Click **🖼 Image** → **upload** one and **reuse** a previously uploaded one; both insert and render; Inline Small/Medium/Full emit the right `![w:…]`; Background mode emits `![bg …]`; picker works on a `/s/{token}` edit link.
  - `GET /api/decks/{id}/assets` is owner-scoped (**404** for a foreign deck); the share variant needs **edit** (a **view** token → 403/404).
  - Every surface zero external requests; light + dark + 375px clean; guide + picker legible in dark mode.
- Adversarial review (owner-scoping + Marp-syntax truth + i18n/a11y/UX) before AND after coding, findings folded.
