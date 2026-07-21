# ADR-0003: PDF export — headless Chromium ใน container ควบคุมจาก Rust

- **Status:** accepted
- **Date:** 2026-07-21
- **Deciders:** Cowork (เสนอเป็น [STD], ไม่ถูกค้านในรอบคุย 2026-07-21)

## Context
ผู้ใช้ต้องการ Export PDF `[SRC]` โดยสไลด์มีฟอนต์ custom (รวมภาษาไทย) และสมการ KaTeX —
PDF ต้องหน้าตาตรงกับที่เห็นบนจอ 100% วิธีเดียวที่ได้ parity จริงคือให้ browser engine ตัวเดียวกับที่ preview เป็นคน render

## Decision
- Image ของแอปติดตั้ง **Chromium** ไว้ด้วย (Debian package, ระบุ path ผ่าน env `CHROME_BIN`)
- Backend (Rust) ใช้ **chromiumoxide** (CDP) เปิดหน้า **print view ของแอปตัวเอง** (`/print/{deck}?token=...`
  ด้วย one-time token — ไม่ต้อง login ใน Chromium)
- หน้า print view render ทุกสไลด์ด้วย Marp Core (ADR-0002) ขนาด 1280×720 แล้ว signal ว่า
  ฟอนต์ + KaTeX พร้อม (`document.fonts.ready` + marker) → Rust สั่ง `Page.printToPDF` ขนาดหน้า = ขนาดสไลด์
- ผลลัพธ์สตรีมกลับให้ผู้ใช้ และ cache ใน `/data/exports` (ลบได้เสมอ)

## Consequences
- **Positive:** PDF ตรงปก 100% รวมฟอนต์ไทย/สมการ/ธีม; ใช้ pipeline เดียวต่อยอดทำ thumbnail เดคได้ฟรี
- **Negative / trade-offs:** image ใหญ่ขึ้น (~250–350MB); ต้อง config Chromium ใน container อย่างระวัง
  (`--no-sandbox` ภายใต้ user ไม่มีสิทธิ์พิเศษ, จำกัด flag, จำกัด URL ที่เปิดได้เป็น localhost ของตัวเองเท่านั้น)
- **Applies to:** Dockerfile (ติดตั้ง chromium), โมดูล export ใน backend, หน้า print view ใน frontend

## Alternatives considered
- ให้ผู้ใช้กด print ใน browser เอง — ผลลัพธ์ต่างกันตาม browser/OS, UX แย่
- typst / weasyprint / printpdf ฝั่ง Rust — render คนละ engine กับ preview → ไม่มีทาง parity กับ CSS ของ Marp
- Node sidecar รัน marp-cli — เพิ่ม runtime Node ทั้งอันเพื่องานเดียว ขัดกับ single-container (ADR-0001)

## Durable contracts
- Chromium ใน container เปิดได้เฉพาะ URL ของแอปตัวเอง (localhost) เท่านั้น — ห้ามรับ URL จากผู้ใช้
- ทุก export ต้องผ่าน one-time token ที่หมดอายุเร็ว ไม่ reuse session cookie ของผู้ใช้
