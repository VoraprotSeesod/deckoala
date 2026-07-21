# โลโก้ Deckoala — บันทึกการคัดเลือก (2026-07-21)

กระบวนการ: สร้าง 4 candidates จากคนละ creative direction → กรรมการ 3 มุมมอง (brand craft / legibility 16px / technical correctness) จัดอันดับแบบ Borda count

**ผู้ชนะ: `ears-monogram` (คะแนน 9)** — หน้าโคอาลาที่ "ใบหน้า = การ์ดสไลด์" (rounded rect) + แผ่นสไลด์ถัดไปโผล่ใต้การ์ด → ไฟล์จริงอยู่ที่ `assets/brand/logo.svg` (และ `logo-dark.svg` สำหรับพื้นเข้ม)

คะแนน: ears-monogram 9, geometric-head 8, koala-hug-deck 8, negative-space-card 5
(กรรมการ craft เลือก ears-monogram, กรรมการ legibility เลือก geometric-head, กรรมการ technical เลือก koala-hug-deck — ears-monogram ชนะเพราะอันดับรวมดีสุดในทุกมุม)

## Candidates ทั้งหมด (เผื่ออยากเปลี่ยนใจ)

### 0 — geometric-head (รองแชมป์)
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect x="16" y="44" width="96" height="54" rx="10" fill="none" stroke="#0B1215" stroke-width="5"/><g fill="#0B1215"><circle cx="39" cy="44" r="16"/><circle cx="89" cy="44" r="16"/><circle cx="64" cy="64" r="27"/></g><g fill="#F8F8FF"><circle cx="40" cy="45" r="6"/><circle cx="88" cy="45" r="6"/><circle cx="49" cy="58" r="4"/><circle cx="79" cy="58" r="4"/><ellipse cx="64" cy="70" rx="8" ry="11"/></g></svg>
```

### 1 — negative-space-card
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><g fill="#0B1215"><circle cx="29" cy="37" r="21"/><circle cx="99" cy="37" r="21"/><rect x="16" y="48" width="96" height="54" rx="10"/><rect x="28" y="107" width="72" height="6" rx="3"/></g><g fill="#F8F8FF"><circle cx="29" cy="37" r="9"/><circle cx="99" cy="37" r="9"/><circle cx="42" cy="66" r="6"/><circle cx="86" cy="66" r="6"/><ellipse cx="64" cy="77" rx="10" ry="14"/></g></svg>
```

### 2 — koala-hug-deck (โคอาลากอดสไลด์แบบกอดต้นไม้ — น่ารักสุดที่ 128px แต่เละที่ 16px)
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><g fill="#0B1215"><circle cx="37" cy="26" r="16"/><circle cx="91" cy="26" r="16"/><circle cx="64" cy="44" r="26"/></g><g fill="#F8F8FF"><circle cx="35" cy="24" r="7"/><circle cx="93" cy="24" r="7"/><circle cx="52" cy="42" r="3"/><circle cx="76" cy="42" r="3"/><ellipse cx="64" cy="50" rx="6.5" ry="8"/></g><rect x="12" y="60" width="104" height="58" rx="10" fill="#0B1215"/><rect x="19" y="67" width="90" height="44" rx="4" fill="#F8F8FF"/><g fill="#0B1215"><rect x="36" y="56" width="10" height="24" rx="5"/><rect x="82" y="56" width="10" height="24" rx="5"/><circle cx="41" cy="78" r="8"/><circle cx="87" cy="78" r="8"/><rect x="36" y="91" width="56" height="6" rx="3"/><rect x="45" y="101" width="38" height="6" rx="3"/></g></svg>
```

### 3 — ears-monogram ✅ ผู้ชนะ
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><g fill="#0B1215"><circle cx="31" cy="33" r="21"/><circle cx="97" cy="33" r="21"/><rect x="20" y="42" width="88" height="58" rx="20"/><rect x="34" y="107" width="60" height="8" rx="4"/></g><g fill="#F8F8FF"><circle cx="31" cy="33" r="9"/><circle cx="97" cy="33" r="9"/><circle cx="44" cy="60" r="5"/><circle cx="84" cy="60" r="5"/><ellipse cx="64" cy="72" rx="11" ry="15"/></g></svg>
```

## หมายเหตุการใช้งาน
- `logo.svg` ออกแบบให้วางบนพื้นสว่าง (ช่องว่างตา/จมูก fill เป็น `#F8F8FF` = สีพื้น brand)
- บนพื้นเข้มใช้ `logo-dark.svg` (สลับสี)
- ใช้เป็น favicon ได้ตรง ๆ (ผ่านการตัดสินเรื่องอ่านออกที่ 16px แล้ว)
