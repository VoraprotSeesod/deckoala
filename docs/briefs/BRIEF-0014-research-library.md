# BRIEF-0014 — Research library → AI source context

- **Status:** ready to build (after BRIEF-0013)
- **Depends on:** BRIEF-0010 (AI generate + `ai.rs`), BRIEF-0004/0007 (upload + magic-byte sniff patterns), BRIEF-0009d (picker/list UI patterns), BRIEF-0013 (Gemini — the research context is provider-agnostic and works for all three)
- **Traces to:** user request 2026-07-23: "อยากให้ Upload ไฟล์งานวิจัยได้ เพราะสไลด์ของผมจะต้องดึงข้อมูลส่วนใหญ่มาจากงานวิจัย." Confirmed decisions: **PDF + .txt/.md**, **server-side text extraction sent to the model as text context** (provider-agnostic, not multimodal upload), and a **central per-user research library reused across decks**.
- **Extended (same session):** "ทำให้ระบบ mcp สามารถสกัดภาพจากเอกสารงานวิจัยแล้ว Upload ภาพเพื่อประกอบ Slide ให้ด้วย" — also **extract embedded images** from a research PDF and let them be **attached to a deck as assets**, reachable over **MCP** so an external AI client can illustrate slides from the paper's own figures (§D below).
- **Scope shape:** backend (migration + one new dependency for PDF text extraction) + frontend (library UI + AI-dialog selection).

## Goal
Upload research documents once into a **personal library**, then when generating slides with AI, **pick which research to draw from** — the server extracts the text and feeds it to the model as source material, so decks are built mostly from the user's research.

## Ground truth (verified)
- `ai.rs::generate` already accepts `prompt` + `existingMarkdown`, builds a `user_content` string, and calls the admin-configured provider (Anthropic/OpenAI/Gemini). It caps `existing` to 32 KB on a char boundary and never lets the client blow the token budget.
- Assets (BRIEF-0004) show the upload pattern: multipart, magic-byte sniff, size cap, `safe_segment` on path parts, files under `<data>/…/`, owner-scoped rows. `research` mirrors this but is **owner-scoped (per user), not per deck**.
- The AI dialog lives in `DeckEditor.svelte` (owner route only, gated by `aiEnabled`).

## Scope

### A. Research library — backend (per-user)
- **Migration `0010_research_docs`**: `id TEXT PK`, `user_id TEXT NOT NULL (FK users.id)`, `original_name TEXT NOT NULL`, `mime TEXT NOT NULL`, `char_count INTEGER NOT NULL`, `created_at TEXT NOT NULL`; index on `user_id`. Additive; `down` drops it.
- **Storage:** the **extracted text** is written to `<data>/research/<user_id>/<id>.txt` (the model only ever needs text; storing the extracted text — not the original binary — keeps it lean and re-usable). `safe_segment` on `user_id`/`id` before building the path.
- **Text extraction** (`research.rs`):
  - **PDF** → text via a Rust PDF-text crate (`pdf-extract` unless it proves unbuildable on the pinned toolchain, then `lopdf`-based extraction). Scanned/image-only PDFs yield little/no text → if the extracted text is empty/whitespace, return **422 "couldn't read text from this PDF (is it a scan?)"** rather than storing an empty doc.
  - **text/markdown** (`.txt`/`.md`, sniffed as UTF-8 text) → stored as-is.
  - Reject anything else (magic-byte / content sniff, mirroring assets) → 415.
  - Caps: upload ≤ **10 MB**; store the extracted text truncated to a bound (e.g. **≤ 500 KB** on a char boundary) so one doc can't be pathological; `char_count` recorded.
- **Endpoints (AuthUser, per-user scoped):**
  - `POST /api/research` (multipart `file`, its own `DefaultBodyLimit`) → 201 `{ id, originalName, mime, charCount, createdAt }`.
  - `GET /api/research` → the user's docs (metadata only — never the full text): `[{ id, originalName, mime, charCount, createdAt }]`.
  - `GET /api/research/{id}/preview` → a short **snippet** (first ~2 KB) of the extracted text, owner-scoped, for the UI preview. Foreign/missing → 404.
  - `DELETE /api/research/{id}` → owner-scoped (double-scoped by id AND user_id), removes the row + the text file. Foreign → 404.
  - Cap the library (e.g. **≤ 50 docs/user**) so it can't grow unbounded.

### B. AI generate uses the selected research (provider-agnostic)
- `GenerateRequest` gains `researchIds: string[]` (optional). The server loads those docs' extracted text **owner-scoped** (a doc not owned by the caller is silently ignored — never another user's text), concatenates them (each prefixed by its `originalName`), and **budgets the total** to a research cap (e.g. **≤ 24 KB** on a char boundary; if several are selected, split the budget) so prompt + existing + research fit the model.
- The prompt becomes, in order: **research context → existing deck (if any) → the user's request**, each clearly delimited, with a system-prompt line telling the model to base the slides on the provided source research and cite figures/claims faithfully (do not invent). Works identically for Anthropic/OpenAI/Gemini (it's just text).
- Keep the existing per-user throttle, semaphore, response cap, timeout and key-hygiene unchanged.

### C. Frontend
- **Research library page `/app/research`** (any signed-in user; the library is per user, not admin): upload (PDF/txt/md), list (name, size/char-count, date), preview snippet, delete. In the nav + command palette (`nav.research`). Modal/route follows the 0009b contract; dark-mode legible; responsive.
- **AI dialog (editor):** when AI is enabled, show the user's research docs as a **checklist**; the chosen `researchIds` go with the generate request. A short hint: "slides will be built from the selected research." If the library is empty, a pointer to `/app/research`.
- All copy `t()` in **both** catalogs (parity test); theme vars; light+dark; responsive.

### D. Figure extraction + MCP (user's extension)
Research papers carry their evidence in figures, so the images must reach the slides too.
- **Extraction at upload:** while parsing a PDF, also pull its **embedded raster images** (PDF `XObject` image streams; PNG/JPEG payloads). Store them beside the text as `<data>/research/<user_id>/<doc_id>/fig-<n>.<ext>`, recorded in a **`research_figures`** table (`id`, `doc_id`, `user_id`, `filename`, `mime`, `width`, `height`, `page`, `created_at`).
  - **Bounds (a PDF is untrusted input):** at most **N figures per document** (e.g. 40), each ≤ **5 MB**, and skip tiny images (< ~64×64 — they are usually icons/rules, not figures). Decoding is local; never trust declared dimensions over the actual bytes (reuse the assets magic-byte sniff).
  - Failure to extract images is **not** an upload failure — the text is what matters; figures are best-effort.
- **Endpoints (owner-scoped):** `GET /api/research/{id}/figures` → `[{ id, url, mime, width, height, page }]`; `GET /research/{user}/{doc}/{file}` served like `/assets` (owner/session-scoped, `safe_segment`, `nosniff`, immutable) for previewing; `POST /api/decks/{deckId}/figures/{figureId}` → **copy the figure into the deck's assets** (reusing `assets::store_asset`'s row/file path) and return the standard `UploadedAsset` so it can be inserted as `![alt](/assets/…)`.
- **MCP tools (BRIEF-0011 pattern, scoped to the TOKEN's user):**
  - `list_research` → the caller's research docs (id, name, charCount).
  - `list_research_figures { researchId }` → that doc's figures (id, page, size) — owner-scoped.
  - `attach_figure { deckId, figureId, alt? }` → copies the figure into that deck's assets and returns the markdown snippet `![alt](/assets/{deckId}/{file})` for the client to place. **Deck AND figure are both owner-checked**; a foreign id is indistinguishable from missing.
  - Tool descriptions state that figures come from the user's own uploaded research.
- **Frontend:** the research page shows a figure gallery per document with an "attach to this deck" action available from the editor's image picker (a **"From research"** tab beside Upload/Reuse), inserting the standard image markdown.
- **Zero-external preserved:** figures are same-origin `/assets/…` after attach; extraction is local; MCP is inbound-only.

### Out of scope
- DOCX/PPTX/other formats (PDF + text only — user's choice); OCR of scanned PDFs; multimodal/native-file upload to the model; sharing research between users; vector/SVG figure reconstruction (raster XObjects only) and figure-caption pairing; RAG/embeddings/chunk-retrieval (whole-text-as-context only, budgeted).

## Business rules
- Research is **owner-scoped**: a user only ever lists/previews/deletes/uses their own docs; the AI context for a generate request is built only from the caller's own `researchIds`.
- **No new outbound-call category:** extraction is fully **local/server-side**; the only outbound call remains the admin-configured LLM endpoint (CLAUDE.md §2 unchanged). Viewer pages stay zero-external.
- The API never returns the full extracted text in bulk (only a bounded preview snippet); nothing here changes deck ownership/soft-delete.
- All persistent state under `/data`.

## Deliverables
Migration `0010_research_docs` + `research_figures` (+ down) · `research.rs` (upload/extract/list/preview/delete + figure extraction, per-user scoped) · PDF-text + image-extraction dependencies (pinned) · `/api/research*` routes incl. figures + serve + attach-to-deck · MCP tools `list_research` / `list_research_figures` / `attach_figure` (`mcp.rs`, token-user scoped) · `GenerateRequest.researchIds` + server-side owner-scoped context assembly + budget in `ai.rs` · `/app/research` page (docs + figure gallery) + nav/palette entry · AI-dialog research checklist · image-picker "From research" tab · TH/EN copy · backend tests (upload+extract a small PDF and a .txt; figures extracted + bounded; owner-scoping: list/preview/delete/figures/attach/generate never cross users; oversized/scanned-empty/wrong-type rejected; MCP tools owner-scoped; migration roundtrip) · vitest for the client wiring.

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): stack healthy + `/api/health`; `cargo test` + fmt + clippy clean; `npm run check` 0/0 + vitest + build; `pr-review` PASS before commit.
- Runtime, in the browser:
  - Upload a small text-based research PDF → it appears in `/app/research` with a sensible char-count and a preview snippet; a `.txt` uploads too; a non-PDF/non-text (e.g. a PNG) is rejected; a scanned/image-only PDF returns the "couldn't read text" error.
  - In the editor's AI dialog (AI configured), select the uploaded research + a prompt → the generated deck visibly reflects the research content; confirm the request carried `researchIds` and the server built the context (a second user's research is never usable — owner-scoped, verified).
  - Delete a research doc → gone from the list and the AI dialog; the text file is removed.
  - Owner-scoping: user A cannot list/preview/delete/generate-from user B's research (404 / ignored).
  - Zero external requests on viewer pages; extraction makes **no** outbound call; light + dark + 375px clean.
- Adversarial review (owner-scoping + extraction safety/DoS + prompt-budget + i18n/UX) before AND after coding, findings folded.
