# ADR-0001: Stack — SvelteKit (static SPA) + Rust/Axum + SQLite ใน container เดียว

- **Status:** accepted
- **Date:** 2026-07-21
- **Deciders:** ผู้ใช้ (framework + Rust BE), Cowork (รายละเอียด)

## Context
Deckoala ต้อง self-host ง่ายด้วย `compose.yml` ไฟล์เดียว `[SRC]`, ใช้ลื่นบนทุกอุปกรณ์ `[SRC]`,
ผู้ใช้เลือก **SvelteKit** เป็น web framework และกำหนดให้ **backend เป็น Rust** `[USER]`

## Decision
- **Frontend:** SvelteKit (Svelte 5 + TypeScript) build ด้วย `adapter-static` → ได้ SPA ล้วน ไม่ต้องมี Node ตอน runtime
- **Backend:** Rust (stable) + **Axum** + tokio; ORM/query ผ่าน **sqlx** (SQLite), รหัสผ่านด้วย **argon2**, session cookie ผ่าน tower-sessions
- **Storage:** **SQLite** ไฟล์เดียวใน volume `/data` (รวม assets, fonts, exports ในโฟลเดอร์เดียวกัน)
- **Runtime:** container เดียว (`deckoala-app`): binary Rust ตัวเดียวเสิร์ฟ (1) static SPA (2) `/api/*` (3) ไฟล์ asset/font (4) ขับ Chromium สำหรับ PDF (ดู ADR-0003)
- **Deploy:** `compose.yml` — 1 service + 1 volume + 1 network; host port ปรับผ่าน `.env` (`DECKOALA_PORT`, default 8321)

## Consequences
- **Positive:** compose สั้นที่สุดเท่าที่เป็นไปได้; backup = copy `/data`; ไม่มี DB container แยก; Rust binary เบาและเร็ว; SPA ทำ editor แบบ interactive ได้เต็มที่
- **Negative / trade-offs:** ไม่มี SSR (ยอมรับได้ — แอปหลัง login, ไม่ต้องการ SEO); การ scale แนวนอนจำกัดด้วย SQLite (พอสำหรับ self-host); ทีมต้องดูแล 2 ภาษา (TS + Rust)
- **Applies to:** โครงสร้าง repo (`frontend/`, `backend/`), Dockerfile, compose.yml

## Alternatives considered
- Node full-stack (SvelteKit adapter-node ทำ API เอง) — ผู้ใช้ต้องการ backend เป็น Rust
- แยก frontend/backend เป็น 2 container — เพิ่ม moving parts โดยไม่ได้อะไรในสเกลนี้
- PostgreSQL — หนักเกินความจำเป็นสำหรับ self-host เดี่ยว; ย้ายทีหลังได้ผ่าน sqlx ถ้าจำเป็นจริง

## Durable contracts
- ทุก state ถาวรอยู่ใต้ `/data` เท่านั้น (DB, assets, fonts, exports) — container ตัวมันเอง stateless
- Backend จอง URL prefix ไว้ 3 ตัว: `/api/` (JSON API), `/assets/` (ไฟล์แนบของเดค), `/fonts/` (ไฟล์ฟอนต์) —
  SPA fallback ห้าม shadow ทั้งสาม prefix นี้; route อื่นทั้งหมดเป็นของ SPA
