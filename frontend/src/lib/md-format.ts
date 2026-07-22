/** Selection-aware Markdown transforms for the formatting toolbar (BRIEF-0012).
 *
 * DECK-CORRUPTION-SENSITIVE, but PURE and DOM-free so it is exhaustively unit
 * tested. Offsets are plain JS-string (UTF-16) offsets — the SAME model
 * CodeMirror uses — so results line up with the editor by construction. Do not
 * re-index to code points.
 *
 * The frontmatter clamp does NOT live here: the caller (applyTransform) clamps
 * the selection to bodyStartOffset before calling, so a click on a never-
 * focused editor (selection {0,0}) can't reach the frontmatter.
 */

export type Sel = { text: string; from: number; to: number };

export type InlineMarker = '**' | '*' | '`' | '~~';

/** When toggling italic `*`, the marker char just outside must not be part of a
 * `**` bold run (and there is no such ambiguity for the other markers). */
function boundaryOk(text: string, openIdx: number, closeAfterIdx: number, marker: InlineMarker): boolean {
	if (marker !== '*') return true;
	return text[openIdx - 1] !== '*' && text[closeAfterIdx] !== '*';
}

export function wrapInline(s: Sel, marker: InlineMarker): Sel {
	const { text } = s;
	const m = marker.length;

	// Empty selection → paired markers with the caret between them.
	if (s.from === s.to) {
		const next = text.slice(0, s.from) + marker + marker + text.slice(s.from);
		return { text: next, from: s.from + m, to: s.from + m };
	}

	// Keep leading/trailing whitespace OUTSIDE the markers (`** x **` won't render).
	const raw = text.slice(s.from, s.to);
	const lead = raw.length - raw.trimStart().length;
	const trail = raw.length - raw.trimEnd().length;
	const from = s.from + lead;
	const to = s.to - trail;
	const core = text.slice(from, to);
	if (core === '') {
		// selection was all whitespace → treat as empty insert at `from`
		const next = text.slice(0, from) + marker + marker + text.slice(from);
		return { text: next, from: from + m, to: from + m };
	}

	// Unwrap when the markers sit immediately OUTSIDE the selection.
	if (
		from >= m &&
		to + m <= text.length &&
		text.slice(from - m, from) === marker &&
		text.slice(to, to + m) === marker &&
		boundaryOk(text, from - m, to + m, marker)
	) {
		const next = text.slice(0, from - m) + core + text.slice(to + m);
		return { text: next, from: from - m, to: to - m };
	}

	// Unwrap when the markers are INSIDE the selection (user selected the markers too).
	if (
		core.length >= 2 * m &&
		core.startsWith(marker) &&
		core.endsWith(marker) &&
		// for '*', the inner run must not actually be '**' bold
		!(marker === '*' && (core[1] === '*' || core[core.length - 2] === '*'))
	) {
		const inner = core.slice(m, core.length - m);
		const next = text.slice(0, from) + inner + text.slice(to);
		return { text: next, from, to: from + inner.length };
	}

	// Wrap.
	const next = text.slice(0, from) + marker + core + marker + text.slice(to);
	return { text: next, from: from + m, to: to + m };
}

// --- line prefixes ----------------------------------------------------------

export type LineKind = 'h1' | 'h2' | 'h3' | 'bullet' | 'numbered' | 'quote';

/** Any single block prefix, stripped before applying a new one (one per line). */
const ANY_PREFIX = /^(#{1,6} |\d+\. |[-*+] |> )/;

const KIND: Record<LineKind, { re: RegExp; add: string }> = {
	h1: { re: /^# /, add: '# ' },
	h2: { re: /^## /, add: '## ' },
	h3: { re: /^### /, add: '### ' },
	bullet: { re: /^[-*+] /, add: '- ' },
	numbered: { re: /^\d+\. /, add: '1. ' },
	quote: { re: /^> /, add: '> ' }
};

/** Thematic break / slide separator — must never get a line prefix, or the
 * slide split changes and two slides silently merge (BRIEF-0012 review). */
const THEMATIC = /^(?:-{3,}|\*{3,}|_{3,})$/;

export function toggleLinePrefix(s: Sel, kind: LineKind): Sel {
	const { text } = s;
	const lineStart = text.lastIndexOf('\n', s.from - 1) + 1;
	// If the selection ends exactly at a line start, don't grab that next line.
	const searchFrom = s.to > s.from && text[s.to - 1] === '\n' ? s.to - 1 : s.to;
	let lineEnd = text.indexOf('\n', searchFrom);
	if (lineEnd === -1) lineEnd = text.length;

	const { re, add } = KIND[kind];
	const lines = text.slice(lineStart, lineEnd).split('\n');
	const skip = (l: string) => l.trim() === '' || THEMATIC.test(l.trim());
	const targeted = lines.filter((l) => !skip(l));
	const allHave = targeted.length > 0 && targeted.every((l) => re.test(l));

	const out = lines.map((l) => {
		if (skip(l)) return l; // leave blanks and slide separators alone
		if (allHave) return l.replace(re, ''); // toggle off
		return add + l.replace(ANY_PREFIX, ''); // strip any family prefix, then add
	});

	const nextBlock = out.join('\n');
	const next = text.slice(0, lineStart) + nextBlock + text.slice(lineEnd);
	return { text: next, from: lineStart, to: lineStart + nextBlock.length };
}

// --- link -------------------------------------------------------------------

export function insertLink(s: Sel): Sel {
	const { text, from, to } = s;
	if (from === to) {
		const ins = '[text](url)';
		const next = text.slice(0, from) + ins + text.slice(to);
		return { text: next, from: from + 1, to: from + 5 }; // select 'text'
	}
	const label = text.slice(from, to);
	const ins = `[${label}](url)`;
	const next = text.slice(0, from) + ins + text.slice(to);
	const urlStart = from + 1 + label.length + 2; // '[' label ']('
	return { text: next, from: urlStart, to: urlStart + 3 }; // select 'url'
}

// --- block insert -----------------------------------------------------------

/** Insert a block on its own lines at/after `pos`, guaranteeing a blank line
 * before it (so a `---` slide break can never be read as a setext H2 underline
 * of the line above) and a newline after. `pos` is pre-clamped to
 * bodyStartOffset by the caller. */
export function insertBlock(text: string, pos: number, block: string): Sel {
	let eol = text.indexOf('\n', pos);
	if (eol === -1) eol = text.length;
	const before = text.slice(0, eol);
	const after = text.slice(eol); // starts with '\n' or is empty

	// Blank line before (skip when the doc is empty / we're already at the top).
	const pre = before.length === 0 ? '' : '\n\n';
	// Blank line AFTER: `after` is '' (EOF) or starts with '\n'. Add a newline
	// unless a blank line already follows (`\n\n`) — otherwise a block like a
	// table would merge the next line into itself (BRIEF-0012 review).
	const post = after === '' || after.startsWith('\n\n') ? '' : '\n';
	const insertion = pre + block + post;
	const next = before + insertion + after;
	const start = before.length + pre.length;
	return { text: next, from: start, to: start + block.length };
}
