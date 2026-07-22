/** Safe reader/writer for a deck's leading YAML frontmatter (BRIEF-0009c).
 *
 * DECK-CORRUPTION-CRITICAL. The theme gallery and the custom-CSS editor both
 * rewrite a single directive; everything else in the document — the slide body,
 * other directives, key order, comments — must survive byte-for-byte.
 *
 * Detection reuses `frontMatterEnd` from slides.ts (the marpit boundary), so
 * "is there frontmatter, and where does it end" can never disagree with how
 * Marp actually parses the deck. Only two directives are ever written: the
 * scalar `theme` and the literal block `style: |`. Everything else is opaque
 * pass-through text — we do NOT emit arbitrary YAML.
 */

import { frontMatterEnd } from './slides';

const MARP_DEFAULT = 'marp: true';

type Region = {
	/** LF-normalized whole document, split into lines. */
	lines: string[];
	/** Interior YAML line indices [start, end) — exclusive of the two `---`
	 * fences — or null when the document has no frontmatter. */
	interior: { start: number; end: number } | null;
};

function locate(lines: string[]): Region {
	const end = frontMatterEnd(lines);
	if (end === -1) return { lines, interior: null };
	return { lines, interior: { start: 1, end } };
}

/** A top-level `key:` line that is active (not indented, not commented). */
function keyLineMatches(line: string, key: string): boolean {
	if (/^\s/.test(line)) return false; // indented → nested, not a directive
	if (/^\s*#/.test(line)) return false; // commented
	return line === `${key}:` || line.startsWith(`${key}:`);
}

/** Value of a scalar directive (first active occurrence), or null. Strips a
 * trailing ` # comment` and surrounding quotes. */
export function getScalar(md: string, key: string): string | null {
	const lines = md.replace(/\r\n/g, '\n').split('\n');
	const { interior } = locate(lines);
	if (!interior) return null;
	for (let i = interior.start; i < interior.end; i++) {
		if (keyLineMatches(lines[i], key)) {
			let value = lines[i].slice(key.length + 1);
			value = value.replace(/\s+#.*$/, '').trim();
			if (
				(value.startsWith('"') && value.endsWith('"')) ||
				(value.startsWith("'") && value.endsWith("'"))
			) {
				value = value.slice(1, -1);
			}
			return value;
		}
	}
	return null;
}

/** True once a line ends the literal block started at `key: |` — i.e. a
 * non-blank line indented no deeper than the key. */
function endsBlock(line: string): boolean {
	return line.trim() !== '' && !/^\s/.test(line);
}

/** Content of a `key: |` literal block as dedented lines, or null. */
export function getBlock(md: string, key: string): string[] | null {
	const lines = md.replace(/\r\n/g, '\n').split('\n');
	const { interior } = locate(lines);
	if (!interior) return null;
	for (let i = interior.start; i < interior.end; i++) {
		if (keyLineMatches(lines[i], key)) {
			const rest = lines[i].slice(key.length + 1).trim();
			if (!rest.startsWith('|') && !rest.startsWith('>')) return null; // not a block
			const body: string[] = [];
			// Indentation of the block is the indentation of its first content line.
			let indent = -1;
			for (let j = i + 1; j < interior.end; j++) {
				if (endsBlock(lines[j])) break;
				const m = lines[j].match(/^(\s*)/);
				const lead = m ? m[1].length : 0;
				if (lines[j].trim() !== '' && indent === -1) indent = lead;
				body.push(indent === -1 ? '' : lines[j].slice(indent));
			}
			// Trim trailing blank lines the block scalar folded in.
			while (body.length && body[body.length - 1] === '') body.pop();
			return body;
		}
	}
	return null;
}

/** Style content for BOTH the `style: |` block form AND the inline
 * `style: "…"` scalar form, as CSS lines. The inline value is unquoted but NOT
 * comment-stripped (CSS hex colours contain `#`). Null when absent. The render
 * sanitizer uses this so an inline `style:` can't slip external url()/@import
 * past the block-only reader (BRIEF-0009c review). */
export function getStyleContent(md: string): string[] | null {
	const block = getBlock(md, 'style');
	if (block) return block;
	const lines = md.replace(/\r\n/g, '\n').split('\n');
	const { interior } = locate(lines);
	if (!interior) return null;
	for (let i = interior.start; i < interior.end; i++) {
		if (keyLineMatches(lines[i], 'style')) {
			let value = lines[i].slice('style'.length + 1).trim();
			if (value.startsWith('|') || value.startsWith('>')) return null; // block handled above
			if (
				(value.startsWith('"') && value.endsWith('"')) ||
				(value.startsWith("'") && value.endsWith("'"))
			) {
				value = value.slice(1, -1);
			}
			return value === '' ? null : [value];
		}
	}
	return null;
}

/** Interior line indices [from, to) spanned by a `key:` directive, block-aware.
 * Returns null when the key is absent. */
function directiveExtent(
	lines: string[],
	interior: { start: number; end: number },
	key: string
): { from: number; to: number } | null {
	for (let i = interior.start; i < interior.end; i++) {
		if (!keyLineMatches(lines[i], key)) continue;
		let to = i + 1;
		const rest = lines[i].slice(key.length + 1).trim();
		if (rest.startsWith('|') || rest.startsWith('>')) {
			while (to < interior.end && !endsBlock(lines[to])) to++;
		}
		return { from: i, to };
	}
	return null;
}

/** Remove every active occurrence of `key` from the interior (block-aware),
 * returning the interior lines that survive. Collapses duplicates. */
function stripDirective(
	lines: string[],
	interior: { start: number; end: number },
	key: string
): string[] {
	const kept: string[] = [];
	let i = interior.start;
	while (i < interior.end) {
		if (keyLineMatches(lines[i], key)) {
			const rest = lines[i].slice(key.length + 1).trim();
			i++;
			if (rest.startsWith('|') || rest.startsWith('>')) {
				while (i < interior.end && !endsBlock(lines[i])) i++;
			}
			continue;
		}
		kept.push(lines[i]);
		i++;
	}
	return kept;
}

function reassemble(
	original: string,
	head: string[],
	interiorLines: string[],
	tailFrom: number
): string {
	const crlf = original.includes('\r\n');
	const lines = original.replace(/\r\n/g, '\n').split('\n');
	const out = [...head, ...interiorLines, ...lines.slice(tailFrom)].join('\n');
	return crlf ? out.replace(/\n/g, '\r\n') : out;
}

/** Build a fresh frontmatter fence for a document that has none, preserving the
 * body exactly. `entryLines` are the interior YAML lines to seed it with. */
function createFrontmatter(md: string, entryLines: string[]): string {
	const crlf = md.includes('\r\n');
	const body = md.replace(/\r\n/g, '\n');
	// No blank line inserted between the fence and the body — the body is
	// preserved byte-for-byte (an extra leading newline would alter slide one).
	const fence = ['---', MARP_DEFAULT, ...entryLines, '---'].join('\n');
	const out = `${fence}\n${body}`;
	return crlf ? out.replace(/\n/g, '\r\n') : out;
}

/** Set (or insert) a scalar directive, e.g. theme. Removes duplicates. */
export function setScalar(md: string, key: string, value: string): string {
	const lines = md.replace(/\r\n/g, '\n').split('\n');
	const { interior } = locate(lines);
	const line = `${key}: ${value}`;
	if (!interior) return createFrontmatter(md, [line]);

	// Drop every existing occurrence, then re-insert once where the first was
	// (or at the end of the interior if it was absent).
	const extent = directiveExtent(lines, interior, key);
	const stripped = stripDirective(lines, interior, key);
	const insertAt = extent ? extent.from - interior.start : stripped.length;
	stripped.splice(insertAt, 0, line);
	return reassemble(md, lines.slice(0, interior.start), stripped, interior.end);
}

/** Set (or insert) a `key: |` literal block from raw CSS lines. The writer owns
 * indentation (2 spaces), so no user line — even `---` or a de-indented one —
 * can break out of the block or read as a slide fence. */
export function setBlock(md: string, key: string, contentLines: string[]): string {
	const lines = md.replace(/\r\n/g, '\n').split('\n');
	const { interior } = locate(lines);
	// Drop a trailing all-blank tail so we don't accumulate empty lines.
	const trimmed = contentLines.slice();
	while (trimmed.length && trimmed[trimmed.length - 1].trim() === '') trimmed.pop();
	// Left-trim every line before the uniform 2-space indent. A YAML literal
	// block takes its indentation from the FIRST content line; if the user's
	// first CSS line were more-indented than a later one, that later line would
	// fall out of the block and corrupt the frontmatter. Flattening to one
	// indent makes the block always valid — CSS whitespace is insignificant.
	const block = [`${key}: |`, ...trimmed.map((l) => (l.trim() === '' ? '' : `  ${l.trimStart()}`))];

	if (!interior) return createFrontmatter(md, block);

	const extent = directiveExtent(lines, interior, key);
	const stripped = stripDirective(lines, interior, key);
	const insertAt = extent ? extent.from - interior.start : stripped.length;
	stripped.splice(insertAt, 0, ...block);
	return reassemble(md, lines.slice(0, interior.start), stripped, interior.end);
}

/** Remove a directive entirely (used to clear custom CSS). */
export function removeDirective(md: string, key: string): string {
	const lines = md.replace(/\r\n/g, '\n').split('\n');
	const { interior } = locate(lines);
	if (!interior) return md;
	const stripped = stripDirective(lines, interior, key);
	return reassemble(md, lines.slice(0, interior.start), stripped, interior.end);
}
