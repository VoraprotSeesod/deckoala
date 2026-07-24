# BRIEF-0013 — Gemini as an AI provider

- **Status:** ready to build
- **Depends on:** BRIEF-0010 (AI settings + `ai.rs` provider adapters + admin UI)
- **Traces to:** user request 2026-07-23: "ต้องการสามารถใช้ API ของ Gemini ในการช่วย Generate สไลด์ได้ด้วย".
- **Scope shape:** small — a third provider adapter in `ai.rs`, one more accepted provider value in `settings.rs`, and the admin UI option. **No migration.** Adds no new *category* of outbound call: Gemini is another instance of the already-permitted "admin-configured LLM endpoint" (CLAUDE.md §2 call #2).

## Goal
An admin can pick **Gemini** in Admin → AI, set the model + API key, and the existing "AI generate" button works against Google's Gemini API — with the same safety posture as the Anthropic/OpenAI adapters.

## Ground truth (verified)
- `ai.rs::generate` branches on `cfg.provider == PROVIDER_ANTHROPIC`; the else branch is the OpenAI-compatible chat-completions shape. It builds the request by hand (reqwest has no `json` feature), sets `redirect::none()`, a timeout, per-user throttle + a global semaphore, caps the response, and **never echoes the provider error/key**.
- `settings.rs` accepts only `anthropic`/`openai` in `put_settings`, defaults provider to anthropic, and validates the base URL is an http(s) URL with a host.
- Frontend `AiProvider = 'anthropic' | 'openai'`; the admin page renders `<option>`s and a base-URL placeholder.

## Scope
### Backend
- Add `PROVIDER_GEMINI = "gemini"` (`settings.rs`); accept it in `put_settings`'s provider validation (now anthropic | openai | gemini).
- **`ai.rs` Gemini branch** — build the request for Google's Generative Language API:
  - URL: `{base_url}/v1beta/models/{model}:generateContent` (default base `https://generativelanguage.googleapis.com`; `join_url` must not mangle the `:generateContent` suffix — build this path explicitly, do not force a `/v1` like the others).
  - **Auth via the `x-goog-api-key` header — NEVER the `?key=` query param** (keys must never go in a URL; privacy rule + our key-hygiene posture).
  - Body: `{ "system_instruction": { "parts": [{ "text": SYSTEM_PROMPT }] }, "contents": [{ "role": "user", "parts": [{ "text": user_content }] }], "generationConfig": { "maxOutputTokens": MAX_TOKENS } }`.
  - Response text: concatenate `candidates[0].content.parts[*].text`; empty → the existing "no content" 502.
  - Reuse the existing timeout / redirect-none / semaphore / response-cap / never-echo-error path unchanged — only the URL, headers, body shape, and response-parse differ.
- Keep the model/base-URL as admin-configured (they can name internal hosts → admin-only, already enforced).

### Frontend
- `AiProvider` gains `'gemini'`; admin page adds the `<option>` (label via `t()`), and shows the Gemini default base URL hint (`https://generativelanguage.googleapis.com`) + a note that the key is a Google AI Studio API key. TH + EN copy; the catalog-parity test guards the keys.

### Out of scope
- Gemini file/multimodal input (that is BRIEF-0014's research feature, done via server-side text extraction and provider-agnostic context — not Gemini-specific uploads); streaming; model auto-discovery.

## Business rules
- Only the **server** calls Gemini, only for a signed-in user's explicit generate request (unchanged from BRIEF-0010). The API key stays write-only (never returned/logged), and the provider error is never surfaced verbatim (it can echo the key).
- The key travels in a **header**, never a URL/query.
- Viewer pages still make zero external requests.

## Deliverables
`PROVIDER_GEMINI` + validation (`settings.rs`) · Gemini branch in `ai.rs::generate` (URL/header/body/parse) · `AiProvider` + admin `<option>` + base-URL hint + TH/EN copy · a unit test for the Gemini URL builder (no `:generateContent` doubling, no key in URL) and the response parser.

## Verification / acceptance gate
- Acceptance gate (CLAUDE.md §4): stack healthy + `/api/health`; `cargo test` + fmt + clippy clean; `npm run check` 0/0 + vitest + build; `pr-review` PASS before commit.
- Unit: the Gemini URL is `…/v1beta/models/{model}:generateContent` with the key ONLY in the `x-goog-api-key` header (assert the built URL contains no key); the parser extracts text from a sample `candidates[...]` body.
- Runtime: as an admin, set provider=Gemini + a model + key in Admin → AI; the AI button generates a deck (or, offline, confirm a Gemini-shaped request is built and the key is header-only, e.g. via a request-inspection unit test). A wrong key → the generic "provider rejected" 502 with **no key leak**. Zero external requests on viewer pages.
- Adversarial review (key-hygiene + request-shape + settings-validation) before AND after coding.
