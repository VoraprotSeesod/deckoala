-- Fonts: instance-level installed fonts (BRIEF-0007). Files live at
-- <DATA_DIR>/fonts/<filename>; served at /fonts/... and declared by
-- /api/fonts.css. See docs/design/ARCHITECTURE.md §5 + §6.5.
--
-- Google Fonts css2 returns one @font-face per unicode-range SUBSET (Thai,
-- Latin, ...) for the same family/weight/style, so the variant key includes
-- unicode_range. Uploads have an empty unicode_range (whole font).
CREATE TABLE fonts (
    id            TEXT PRIMARY KEY,
    family        TEXT NOT NULL,
    weight        TEXT NOT NULL DEFAULT '400',
    style         TEXT NOT NULL DEFAULT 'normal',
    unicode_range TEXT NOT NULL DEFAULT '',
    filename      TEXT NOT NULL,
    format        TEXT NOT NULL,
    source        TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_fonts_variant ON fonts (family, weight, style, unicode_range);
CREATE INDEX idx_fonts_family ON fonts (family);
