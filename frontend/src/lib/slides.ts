// Deck ↔ slide segmentation for the thumbnail rail and drag-reorder (BRIEF-0004).
//
// DECK-CORRUPTION-CRITICAL: the rail maps thumbnails (one Marp SVG per real
// slide) to reorder indices (this slide array). If the segmentation disagrees
// with Marp's slide split, a drag moves the wrong block and silently corrupts
// the deck. So boundaries are derived from the SAME markdown-it instance Marp
// renders with (via parseTokens) — a setext underline (`text\n---`), a `---`
// inside an HTML comment, and a `---` inside indented/fenced code are NOT
// slide breaks, exactly as Marp treats them.
//
// splitDeck / joinDeck operate on LF-normalized text; reorderSlides preserves
// the original deck's CRLF so a reorder never rewrites untouched line endings.

import { parseTokens } from './marp';

export type SplitResult = {
	/** The leading `---\n…\n---` block, or '' when the deck has no front matter. */
	frontMatter: string;
	/** Slide bodies, in order (one entry per real Marp slide). */
	slides: string[];
};

/** Index of the front-matter closing delimiter line, or -1. A lone leading
 * `---` with no `---`/`...` close is body, not front matter. Exported so the
 * frontmatter writer (BRIEF-0009c) shares the SAME boundary Marpit uses —
 * re-deriving it elsewhere is how a deck gets corrupted. */
export function frontMatterEnd(lines: string[]): number {
	// Fences are column-0 only (trailing space allowed). Matching `.trim()`
	// would wrongly close the block at an INDENTED `---` line living inside a
	// `style: |` literal block, so anchor to the line start (BRIEF-0009c).
	if (lines.length === 0 || !/^---\s*$/.test(lines[0])) return -1;
	for (let i = 1; i < lines.length; i++) {
		if (/^(?:---|\.\.\.)\s*$/.test(lines[i])) return i;
	}
	return -1;
}

/** Character offset where the slide body begins — just past the frontmatter
 * fence (and its newline), or 0 when there is none. Inserts clamp to this so a
 * snippet dropped while the caret sits at offset 0 (a freshly-loaded, never-
 * focused editor) can't land ahead of the `---` fence and break frontmatter
 * parsing (BRIEF-0009d). Works for LF or CRLF (line length includes the `\r`). */
export function bodyStartOffset(md: string): number {
	const lines = md.split('\n');
	const fmEnd = frontMatterEnd(lines);
	if (fmEnd === -1) return 0;
	let offset = 0;
	for (let i = 0; i <= fmEnd; i++) offset += lines[i].length + 1; // +1 for the '\n'
	return Math.min(offset, md.length);
}

/** Split an LF-normalized deck into front matter + slide bodies. */
export function splitDeck(md: string): SplitResult {
	const lines = md.split('\n');

	const fmEnd = frontMatterEnd(lines);
	let frontMatter = '';
	let bodyStart = 0;
	if (fmEnd !== -1) {
		frontMatter = lines.slice(0, fmEnd + 1).join('\n');
		bodyStart = fmEnd + 1;
	}
	const bodyLines = lines.slice(bodyStart);
	const body = bodyLines.join('\n');

	// Marpit emits one `marpit_slide_open` (section) token per REAL slide, so
	// the count and boundaries match what Marp renders exactly (a setext
	// underline, or `---` inside code/comments, never opens a new section).
	// Each slide after the first opens at its separator line (`.map[0]`).
	const slideStarts = parseTokens(body)
		.filter((t) => t.type === 'marpit_slide_open' && t.map)
		.map((t) => (t.map as [number, number])[0]);
	const boundaryLines = slideStarts.slice(1);

	const slides: string[] = [];
	let start = 0;
	for (const boundary of boundaryLines) {
		slides.push(bodyLines.slice(start, boundary).join('\n'));
		start = boundary + 1; // the separator line itself is not slide content
	}
	slides.push(bodyLines.slice(start).join('\n'));

	return { frontMatter, slides };
}

/** Rejoin front matter + slides into LF-normalized markdown. */
export function joinDeck(frontMatter: string, slides: string[]): string {
	const body = slides.join('\n---\n');
	return frontMatter ? `${frontMatter}\n${body}` : body;
}

/** Move the slide at `from` to index `to`, returning the rewritten markdown.
 * Front matter and every slide's content are preserved exactly, and the
 * deck's original line endings (LF or CRLF) are kept. */
export function reorderSlides(md: string, from: number, to: number): string {
	const crlf = md.includes('\r\n');
	const lf = crlf ? md.replace(/\r\n/g, '\n') : md;

	const { frontMatter, slides } = splitDeck(lf);
	if (
		from === to ||
		from < 0 ||
		to < 0 ||
		from >= slides.length ||
		to >= slides.length
	) {
		return md;
	}
	const next = slides.slice();
	const [moved] = next.splice(from, 1);
	next.splice(to, 0, moved);

	const out = joinDeck(frontMatter, next);
	return crlf ? out.replace(/\n/g, '\r\n') : out;
}
