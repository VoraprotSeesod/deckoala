# ADR-0002: Slide engine — Marp Core, render ฝั่ง browser

- **Status:** accepted
- **Date:** 2026-07-21
- **Deciders:** Cowork (ผู้ใช้มอบอำนาจการเลือก engine โดยมีเงื่อนไข backend = Rust)

## Context
ต้องแปลง Markdown (+ LaTeX) เป็นสไลด์ `[SRC]` พร้อม live preview `[SRC]`, present ผ่านเว็บ `[SRC]`,
export PDF ที่หน้าตาตรงกับจอ `[SRC]` และธีม default `#F8F8FF`/`#0B1215` `[SRC]`
เงื่อนไขสำคัญ: backend เป็น Rust `[USER]` — slide engine ที่โตแล้วล้วนเป็น JavaScript
ดังนั้นตัว engine ต้องรันใน **browser** ไม่ใช่ฝั่ง server (ไม่เอา Node sidecar เพราะเพิ่ม container/process โดยไม่จำเป็น)

## Decision
ใช้ **`@marp-team/marp-core`** bundle ไปกับ SvelteKit และ render ฝั่ง browser ทั้ง 3 จุด:
1. **Preview** ใน editor (render ใหม่แบบ debounce ขณะพิมพ์)
2. **Present mode** (fullscreen + คีย์ลัด)
3. **Print view** (`/print/...` — หน้าเดียวเรียงทุกสไลด์ ให้ Chromium พิมพ์เป็น PDF ตาม ADR-0003)

Backend เก็บ Markdown ดิบเท่านั้น ไม่เคย render สไลด์เอง → renderer มีตัวเดียว ผลลัพธ์ตรงกันทุกที่ (WYSIWYG)

ธีม default ของ instance = Marp CSS theme ชื่อ `deckoala` (พื้น `#F8F8FF`, อักษร `#0B1215`, ฟอนต์รองรับไทย)

## Consequences
- **Positive:** syntax เป็นมาตรฐาน Marp (คั่นสไลด์ด้วย `---`, directives, image syntax ขยาย); **KaTeX มีในตัว** ตอบโจทย์ LaTeX `[SRC]`; ธีมเป็น CSS ล้วน — รองรับ custom font/สีได้ตรง ๆ; speaker notes ผ่าน HTML comment ได้ทันที; เอกสาร/ชุมชนใหญ่
- **Negative / trade-offs:** ไม่มี server-side render → ทำ thumbnail/OG preview ต้องพึ่ง Chromium (มีอยู่แล้วจาก ADR-0003); interactive fragment ขั้นสูงสู้ Reveal.js ไม่ได้ (ยอมรับ — ไม่อยู่ใน requirement)
- **Applies to:** frontend package.json, โมดูล preview/present/print, การออกแบบธีม

## Alternatives considered
- **Reveal.js** — present mode แข็งแรง แต่ syntax ปน HTML มากกว่า และ pipeline PDF ต้องประกอบเองเยอะกว่า
- **Slidev** — ออกแบบมาให้ dev รันในเครื่องตัวเอง ผูก Vue ทั้งที่เราใช้ Svelte
- **เขียน renderer เอง** (markdown-it + layout เอง) — คุมได้สุดแต่แพงสุด; ค่อยพิจารณาอีกทีเมื่อถึงเฟส visual editor เต็มรูปแบบ

## Durable contracts
- **ไฟล์เดค = Marp Markdown มาตรฐาน** — ผู้ใช้ import/export `.md` แล้วไปเปิดกับ marp-cli ที่อื่นได้ (ไม่มี lock-in)
- สไลด์คั่นด้วย `---`; speaker notes เป็น HTML comment ตามธรรมเนียม Marp
