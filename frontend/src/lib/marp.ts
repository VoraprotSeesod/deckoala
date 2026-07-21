import { Marp } from '@marp-team/marp-core';
import { themeDeckoala } from './theme-deckoala';

// One shared renderer for preview / present / print (ADR-0002: a single
// client-side pipeline keeps everything WYSIWYG).
// - html: false → raw HTML in markdown stays escaped (XSS defense; decks
//   get shared cross-user in BRIEF-0008). Locked by BRIEF-0003.
// - katexFontPath points at our own static assets — marp-core's default is
//   a CDN, which would violate the zero-external-request invariant.
//
// NOTE: KaTeX's CSS is imported by the editor component (not here) so this
// module stays free of CSS side-effects and is importable in a plain Node
// context (vitest uses it for slide segmentation).
const marp = new Marp({
	html: false,
	math: { lib: 'katex', katexFontPath: '/katex-fonts/' },
	inlineSVG: true
});
marp.themeSet.add(themeDeckoala);

export type RenderedDeck = {
	html: string;
	css: string;
	slideCount: number;
	/** Speaker notes per slide (Marp HTML comments; directive comments like
	 * `<!-- _class: lead -->` are excluded by marp-core). '' when a slide has
	 * no notes. Rendered as plain text — never as HTML. */
	notes: string[];
};

export function renderDeck(markdown: string): RenderedDeck {
	const { html, css, comments } = marp.render(markdown);
	return {
		html,
		css,
		slideCount: comments.length,
		notes: comments.map((slideComments) => slideComments.join('\n\n'))
	};
}

/** Tokenize markdown with the SAME markdown-it instance Marp uses, so slide
 * segmentation matches Marp's slide split exactly. Marpit emits one
 * `marpit_slide_open` token per real slide (BRIEF-0004). */
export function parseTokens(
	markdown: string
): Array<{ type: string; level: number; map: [number, number] | null }> {
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	return (marp as any).markdown.parse(markdown, {});
}
