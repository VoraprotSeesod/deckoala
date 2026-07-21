import { describe, expect, it } from 'vitest';
import { splitDeck, joinDeck, reorderSlides } from './slides';
import { renderDeck } from './marp';

const FM = '---\nmarp: true\ntheme: deckoala\n---';

/** The load-bearing invariant: our segmentation must agree with Marp's. */
function expectMatchesMarp(md: string) {
	expect(splitDeck(md).slides.length).toBe(renderDeck(md).slideCount);
}

describe('splitDeck matches Marp slide count', () => {
	it('front matter + explicit --- separators', () => {
		const md = `${FM}\n# One\n\n---\n\n# Two\n\n---\n\n# Three`;
		expect(splitDeck(md).slides).toHaveLength(3);
		expectMatchesMarp(md);
	});

	it('setext H2 underline is NOT a slide break', () => {
		// "Intro" underlined by --- is an <h2>, one slide — a naive line rule
		// would wrongly split this into two.
		const md = `${FM}\nIntro\n---\nbody of the same slide`;
		expect(splitDeck(md).slides).toHaveLength(1);
		expectMatchesMarp(md);
	});

	it('--- inside a fenced code block is NOT a slide break', () => {
		const md = `${FM}\n# Code\n\n\`\`\`yaml\nfoo: 1\n---\nbar: 2\n\`\`\`\n\n---\n\n# Next`;
		expect(splitDeck(md).slides).toHaveLength(2);
		expect(splitDeck(md).slides[0]).toContain('bar: 2');
		expectMatchesMarp(md);
	});

	it('--- inside an indented code block is NOT a slide break', () => {
		const md = `${FM}\n# Indented\n\n    code line\n    ---\n    more code\n\n---\n\n# Next`;
		expect(splitDeck(md).slides).toHaveLength(2);
		expectMatchesMarp(md);
	});

	it('*** and ___ thematic breaks split like Marp does', () => {
		const md = `${FM}\n# A\n\n***\n\n# B\n\n___\n\n# C`;
		expectMatchesMarp(md);
	});
});

describe('front matter handling', () => {
	it('detects a closed --- front-matter block', () => {
		expect(splitDeck(`${FM}\n# Body`).frontMatter).toBe(FM);
	});

	it('treats a lone leading --- (no close) as body, not front matter', () => {
		expect(splitDeck('# No front matter\n\ncontent').frontMatter).toBe('');
	});
});

describe('round-trip (LF)', () => {
	it('front matter + multiple slides', () => {
		const md = `${FM}\n# One\n\n---\n\n# Two\n\n---\n\n# Three\n`;
		const { frontMatter, slides } = splitDeck(md);
		expect(joinDeck(frontMatter, slides)).toBe(md);
	});

	it('no front matter', () => {
		const md = '# One\n\n---\n\n# Two';
		const { frontMatter, slides } = splitDeck(md);
		expect(joinDeck(frontMatter, slides)).toBe(md);
	});

	it('single slide', () => {
		const md = `${FM}\n# Only\n`;
		const { frontMatter, slides } = splitDeck(md);
		expect(joinDeck(frontMatter, slides)).toBe(md);
	});
});

describe('reorderSlides', () => {
	const md = `${FM}\n# A\n\n---\n\n# B\n\n---\n\n# C`;

	it('moves the first slide to last', () => {
		const { slides } = splitDeck(reorderSlides(md, 0, 2));
		expect(slides.map((s) => s.trim())).toEqual(['# B', '# C', '# A']);
	});

	it('moves the last slide to first', () => {
		const { slides } = splitDeck(reorderSlides(md, 2, 0));
		expect(slides.map((s) => s.trim())).toEqual(['# C', '# A', '# B']);
	});

	it('preserves front matter and slide count', () => {
		const { frontMatter, slides } = splitDeck(reorderSlides(md, 1, 2));
		expect(frontMatter).toBe(FM);
		expect(slides).toHaveLength(3);
	});

	it('is a no-op for from === to or out-of-range indices', () => {
		expect(reorderSlides(md, 1, 1)).toBe(md);
		expect(reorderSlides(md, 5, 0)).toBe(md);
		expect(reorderSlides(md, 0, 9)).toBe(md);
	});

	it('never drops content when reordering a fenced-code deck', () => {
		const withFence = `${FM}\n# A\n\n\`\`\`\n---\n\`\`\`\n\n---\n\n# B`;
		const out = reorderSlides(withFence, 0, 1);
		expect(splitDeck(out).slides).toHaveLength(2);
		expect(out).toContain('```\n---\n```');
		expect(out).toContain('# A');
		expect(out).toContain('# B');
	});

	it('preserves CRLF line endings', () => {
		const crlf = `${FM}\n# A\n\n---\n\n# B`.replace(/\n/g, '\r\n');
		const out = reorderSlides(crlf, 0, 1);
		expect(out.includes('\r\n')).toBe(true);
		expect(out.includes('\n\n')).toBe(false); // no bare-LF leaked in
		// content preserved, order swapped
		expect(out).toContain('# A');
		expect(out).toContain('# B');
		expect(out.indexOf('# B')).toBeLessThan(out.indexOf('# A'));
	});
});
