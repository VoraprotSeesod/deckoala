import { describe, it, expect } from 'vitest';
import { wrapInline, toggleLinePrefix, insertLink, insertBlock, type Sel } from './md-format';

const sel = (text: string, from: number, to: number): Sel => ({ text, from, to });

describe('wrapInline', () => {
	it('wraps a selection and keeps it covering the same text', () => {
		const r = wrapInline(sel('a word here', 2, 6), '**');
		expect(r.text).toBe('a **word** here');
		expect(r.text.slice(r.from, r.to)).toBe('word');
	});

	it('inserts paired markers with the caret between on an empty selection', () => {
		const r = wrapInline(sel('ab', 1, 1), '**');
		expect(r.text).toBe('a****b');
		expect(r.from).toBe(3);
		expect(r.to).toBe(3);
	});

	it('unwraps when markers are just outside the selection', () => {
		// 'word' is at offsets 4..8 in 'a **word** b'.
		const r = wrapInline(sel('a **word** b', 4, 8), '**');
		expect(r.text).toBe('a word b');
		expect(r.text.slice(r.from, r.to)).toBe('word');
	});

	it('unwraps when the markers are inside the selection', () => {
		const r = wrapInline(sel('a **word** b', 2, 10), '**');
		expect(r.text).toBe('a word b');
	});

	it('italic never eats a star off bold text', () => {
		// selecting `word` inside **word** and applying italic → bold+italic, not corruption
		const r = wrapInline(sel('**word**', 2, 6), '*');
		expect(r.text).toBe('***word***');
	});

	it('unwraps italic without touching adjacent bold', () => {
		const r = wrapInline(sel('*word*', 1, 5), '*');
		expect(r.text).toBe('word');
	});

	it('keeps leading/trailing whitespace outside the markers', () => {
		const r = wrapInline(sel('x  word  y', 1, 9), '**');
		expect(r.text).toBe('x  **word**  y');
	});

	it('handles inline code and strikethrough', () => {
		expect(wrapInline(sel('run cmd now', 4, 7), '`').text).toBe('run `cmd` now');
		expect(wrapInline(sel('old text', 0, 3), '~~').text).toBe('~~old~~ text');
	});

	it('agrees with UTF-16 offsets across an astral character', () => {
		// '🎉' is two UTF-16 units; CodeMirror offsets count units, and so must we.
		const text = '🎉 word';
		const from = 3; // '🎉 ' = 2 + 1 = 3 units
		const r = wrapInline(sel(text, from, from + 4), '**');
		expect(r.text).toBe('🎉 **word**');
	});
});

describe('toggleLinePrefix', () => {
	it('adds and removes a heading', () => {
		const on = toggleLinePrefix(sel('Title', 0, 0), 'h2');
		expect(on.text).toBe('## Title');
		const off = toggleLinePrefix(on, 'h2');
		expect(off.text).toBe('Title');
	});

	it('is heading-exclusive: h2 replaces an existing h1', () => {
		expect(toggleLinePrefix(sel('# Title', 0, 0), 'h2').text).toBe('## Title');
	});

	it('switches bullet to numbered without stacking markers', () => {
		expect(toggleLinePrefix(sel('- item', 0, 0), 'numbered').text).toBe('1. item');
	});

	it('detects and toggles off a real ordered list (2. / 3.)', () => {
		const md = '1. a\n2. b\n3. c';
		const off = toggleLinePrefix(sel(md, 0, md.length), 'numbered');
		expect(off.text).toBe('a\nb\nc');
	});

	it('applies across a multi-line selection and skips blank lines', () => {
		const md = 'a\n\nb';
		const r = toggleLinePrefix(sel(md, 0, md.length), 'bullet');
		expect(r.text).toBe('- a\n\n- b');
	});

	it('toggles a quote', () => {
		expect(toggleLinePrefix(sel('note', 0, 0), 'quote').text).toBe('> note');
	});

	it('never prefixes a --- slide separator (would merge two slides)', () => {
		const md = '# A\n\n---\n\n# B';
		const r = toggleLinePrefix(sel(md, 0, md.length), 'bullet');
		// Headings become bullets (one prefix per line); the --- separator is left alone.
		expect(r.text).toBe('- A\n\n---\n\n- B');
		expect(r.text).toContain('\n---\n'); // slide split intact
	});

	it('does not grab the next line when the selection ends at a line boundary', () => {
		// selection covers 'a\n' (ends at the start of line 'b'); only 'a' is prefixed.
		const md = 'a\nb';
		const r = toggleLinePrefix(sel(md, 0, 2), 'bullet');
		expect(r.text).toBe('- a\nb');
	});
});

describe('insertLink', () => {
	it('wraps a selection and selects the url', () => {
		const r = insertLink(sel('see here now', 4, 8));
		expect(r.text).toBe('see [here](url) now');
		expect(r.text.slice(r.from, r.to)).toBe('url');
	});

	it('inserts a placeholder link and selects the text on an empty selection', () => {
		const r = insertLink(sel('', 0, 0));
		expect(r.text).toBe('[text](url)');
		expect(r.text.slice(r.from, r.to)).toBe('text');
	});
});

describe('insertBlock', () => {
	const deck = '---\nmarp: true\ntheme: deckoala\n---\n\n# Slide\n';
	const bodyStart = deck.indexOf('# Slide'); // caller passes max(caret, bodyStartOffset)

	it('places a block on its own lines with a blank line before it', () => {
		const r = insertBlock(deck, deck.length, '---');
		// The slide-break is preceded by a blank line, so it is NOT a setext underline.
		expect(r.text).toMatch(/\n\n---\n?$/);
		expect(r.text.slice(r.from, r.to)).toBe('---');
	});

	it('never merges into the current line (no setext H2)', () => {
		const md = '# Slide\nsome text';
		const r = insertBlock(md, md.length, '---');
		expect(r.text).toBe('# Slide\nsome text\n\n---');
		// "some text\n---" (setext) must NOT appear.
		expect(r.text).not.toMatch(/some text\n---/);
	});

	it('adds a blank line AFTER the block when a non-blank line follows (no table merge)', () => {
		// A table inserted before body text must not absorb that text as a row.
		const md = '# My Slide\nIntro sentence.';
		const r = insertBlock(md, 0, '| A | B |\n| --- | --- |\n| 1 | 2 |');
		expect(r.text).toBe('# My Slide\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\nIntro sentence.');
		// A blank line separates the table from the following paragraph.
		expect(r.text).toMatch(/\| 1 \| 2 \|\n\nIntro sentence\./);
	});

	it('does not add a second blank line when one already follows', () => {
		const md = '# A\n\nbody';
		const r = insertBlock(md, 0, '---');
		expect(r.text).toBe('# A\n\n---\n\nbody');
	});

	it('adds no trailing newline at end of document', () => {
		expect(insertBlock('# Slide\nsome text', '# Slide\nsome text'.length, '---').text).toBe(
			'# Slide\nsome text\n\n---'
		);
	});

	it('inserts a block after the frontmatter when clamped to bodyStart', () => {
		const r = insertBlock(deck, bodyStart, '$$\nE = mc^2\n$$');
		expect(r.text.startsWith('---\nmarp: true\ntheme: deckoala\n---')).toBe(true);
		expect(r.text).toContain('$$\nE = mc^2\n$$');
	});
});
