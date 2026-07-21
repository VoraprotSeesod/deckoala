# REQ-ANALYSIS-v1 — Deckoala

> **สถานะ: ร่าง v1** — รอคุยกับผู้ใช้เพื่อปิด open questions
> Source หลัก: [user-brief-2026-07-21](../raw/user-brief-2026-07-21.md)
> Tag ที่ใช้: `[SRC]` = มาจากคำอธิบายผู้ใช้โดยตรง, `[USER]` = ผู้ใช้ตัดสินใจระหว่างคุย, `[STD]` = แนวปฏิบัติมาตรฐานที่เสนอให้อนุมัติ

---

## 1. ภาพรวม

**Deckoala** — เว็บแอปสร้างสไลด์นำเสนอจาก Markdown (self-hosted)
ผู้ใช้เขียน Markdown/LaTeX เห็น preview สด ๆ, กด Present ผ่านเว็บ, export PDF ได้
Deploy ด้วย `compose.yml` ไฟล์เดียว โดเมนเป้าหมาย: `deckoala.dimenshade.com` `[SRC]`

## 2. โมดูล / ความสามารถ

| # | โมดูล | รายละเอียด | ที่มา |
|---|---|---|---|
| M1 | **Editor** | เขียน Markdown + LaTeX (สมการ), มี Drag & Drop ช่วยสร้างสไลด์ | `[SRC]` |
| M2 | **Live Preview** | เห็นสไลด์ระหว่างพิมพ์แบบเรียลไทม์ (split pane) | `[SRC]` |
| M3 | **Present mode** | นำเสนอเต็มจอผ่านเบราว์เซอร์ (คีย์ลัดเปลี่ยนสไลด์) | `[SRC]` |
| M4 | **PDF Export** | ดาวน์โหลดเดคเป็น PDF ตรงตามที่เห็นบนจอ | `[SRC]` |
| M5 | **Font Manager** | อัปโหลดฟอนต์เอง (.ttf/.woff2) + ดึงจากแหล่งภายนอก (เช่น Google Fonts) มาติดตั้งง่าย ๆ | `[SRC]` |
| M6 | **File Management** | บันทึก, ทำสำเนา (duplicate), จัดการเดคผ่านเว็บทั้งหมด | `[SRC]` |
| M7 | **Sharing** | แชร์เดคให้คนอื่น | `[SRC]` |
| M8 | **Theming** | ค่า default: พื้นหลัง `#F8F8FF`, ตัวอักษร `#0B1215` | `[SRC]` |
| M9 | **Self-host** | `compose.yml` ไฟล์เดียว ติดตั้งง่าย | `[SRC]` |
| M10 | **Responsive** | ใช้งานสะดวกทุกอุปกรณ์ (desktop / tablet / mobile) | `[SRC]` |
| M11 | **Branding** | โลโก้ Deckoala (โคอาลา + เดค) ใช้ในเว็บ | `[SRC]` |

## 3. Entities (ร่างแรก)

```
User ───< Deck ───< Asset (รูป/ไฟล์ที่อัปโหลดใช้ในเดค)
              ───< ShareLink (token, สิทธิ์ view/edit)
              ───< Revision (ประวัติ/สำรองอัตโนมัติ)  [STD เสนอ]
Font  (ระดับ instance หรือระดับ user — รอตัดสินใจ)
```

- **Deck** = ไฟล์ Markdown 1 ไฟล์ (สไลด์คั่นด้วย `---` ตามธรรมเนียม Marp/มาตรฐาน) + metadata (ชื่อ, ธีม, ฟอนต์)
- **Asset** = รูปภาพ/ไฟล์แนบ ลากวางลง editor ได้
- **ShareLink** = ลิงก์แชร์แบบ token สำหรับให้คนอื่นดู (หรือแก้ไข)

## 4. Business rules ที่ชัดแล้ว

- BR1: สีธีมเริ่มต้น — พื้นหลัง `#F8F8FF` (Ghost White), ตัวอักษร `#0B1215` (ใกล้ Rich Black) `[SRC]`
- BR2: ทุกการจัดการไฟล์ (บันทึก/สำเนา) ทำผ่านเว็บ ไม่ต้องพึ่งเครื่องมือภายนอก `[SRC]`
- BR3: ระบบต้องรันได้ครบด้วย `docker compose up` จาก compose.yml ไฟล์เดียว `[SRC]`
- BR4: รองรับ LaTeX (สมการคณิตศาสตร์) ใน Markdown `[SRC]`

## 5. ข้อเสนอมาตรฐาน `[STD]` (ใช้เลยถ้าไม่ค้าน)

- S1: **Storage = SQLite** ไฟล์เดียวใน volume — self-host ง่ายสุด ไม่ต้องมี container DB แยก
- S2: **PDF export ฝั่ง server** ด้วย headless Chromium ใน container → PDF ตรงกับที่เห็นบนจอ 100% (ฟอนต์/สมการครบ)
- S3: **Autosave** ระหว่างพิมพ์ + เก็บ revision ย้อนหลังแบบง่าย
- S4: **ฟอนต์ default ต้องรองรับภาษาไทย** (เช่น Noto Sans Thai / Sarabun) นอกเหนือจากละติน
- S5: ภาษาเอกสารโปรเจกต์ — คุย/วิเคราะห์เป็นไทย, brief + โค้ด + identifier เป็นอังกฤษ
- S6: LaTeX ผ่าน **KaTeX** (เร็ว, render ฝั่ง client ได้, marp-core มีในตัว)

## 6. Open Questions — สถานะหลังคุยรอบที่ 1 (2026-07-21)

| # | คำถาม | คำตอบ | ที่มา |
|---|---|---|---|
| Q1 | Web framework? | **SvelteKit** | `[USER]` |
| Q2 | Slide engine? | ผู้ใช้กำหนดเพิ่ม: **backend ต้องเป็น Rust** และมอบให้ผู้ออกแบบเลือก engine → เลือก **Marp Core (render ฝั่ง browser)** — ดู [ADR-0002](../../../decisions/ADR-0002-slide-engine.md) | `[USER]` + delegation |
| Q3 | ขอบเขต Drag & Drop? | **เฟสแรกแบบง่าย** (จัดลำดับสไลด์ + ลากไฟล์ลง editor) แล้วค่อยต่อยอด visual editor ใน roadmap | `[USER]` |
| Q4 | ระบบบัญชี + การแชร์? | **Multi-user + share link** (สิทธิ์ view/edit ต่อลิงก์) | `[USER]` |
| Q5 | ฟอนต์ระดับ instance หรือระดับ user? | ใช้ค่า default: **ระดับ instance** — ยังค้านได้ก่อนถึง brief ของ Font Manager | `[STD]` |
| Q6 | Speaker notes / presenter view? | ใช้ค่า default: **มี** (Marp เก็บ note ใน HTML comment ได้อยู่แล้ว) — presenter view อยู่ roadmap | `[STD]` |
| Q7 | ภาษา UI ของแอป? | ค่า default ที่เสนอ: **UI อังกฤษก่อน + i18n ไทยใน roadmap** — ยังค้านได้ | `[STD]` |

## 9. บันทึกการตัดสินใจ stack (2026-07-21)

- Frontend: **SvelteKit** (adapter-static → SPA) `[USER]`
- Backend: **Rust** (Axum) — เสิร์ฟทั้ง static frontend + REST API จบใน container เดียว `[USER]` + `[STD]`
- Slide engine: **Marp Core ฝั่ง browser** — preview / present / print ใช้ renderer ตัวเดียวกัน → WYSIWYG ตรงกันทุกที่ `[STD]` (ผู้ใช้มอบให้เลือก)
- Storage: **SQLite** (sqlx) `[STD]` — ไม่ถูกค้าน
- PDF: **headless Chromium ใน container ควบคุมจาก Rust** (chromiumoxide) `[STD]`
- รายละเอียดทั้งหมด: [ADR-0001](../../../decisions/ADR-0001-stack.md), [ADR-0002](../../../decisions/ADR-0002-slide-engine.md), [ADR-0003](../../../decisions/ADR-0003-pdf-export.md), [ARCHITECTURE.md](../../design/ARCHITECTURE.md)

## 7. ข้อเสนอแนะเพิ่มเติม

**รับเข้าแผนแล้ว (มีที่ลงใน roadmap):**
- Import/Export ไฟล์ `.md` — ผูกกับ durable contract ของ [ADR-0002](../../../decisions/ADR-0002-slide-engine.md) (BRIEF-0002) `[STD]`
- Speaker notes + Presenter view (หน้าจอผู้พูดเห็น note + สไลด์ถัดไป) — BRIEF-0005 `[STD]`
- Visual editor เต็มรูปแบบ (phase 2 ตามคำตอบ Q3) — BRIEF-0010 `[USER]`

**nice-to-have — ยังรอผู้ใช้เลือก (ยังไม่อยู่ใน roadmap):**
- Theme gallery + custom CSS ต่อเดค
- PWA / ใช้งาน offline บางส่วน
- Realtime collaboration (แก้พร้อมกันหลายคน) — แนะนำเป็น phase หลัง เพราะซับซ้อนสูง
- คีย์ลัดครบชุด + command palette
- Dark mode ของตัว UI (ไม่กระทบธีมสไลด์)

## 8. สิ่งที่ต้องส่งมอบนอกเหนือจากโค้ด

- โลโก้ Deckoala (SVG) ใช้สี brand `#F8F8FF` / `#0B1215` `[SRC]` — ทำตอนวาง scaffolding
- compose.yml + README วิธี self-host (รวม reverse proxy ชี้ `deckoala.dimenshade.com`)
