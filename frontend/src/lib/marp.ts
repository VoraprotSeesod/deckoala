import { Marp } from '@marp-team/marp-core';
import { themeDeckoala } from './theme-deckoala';
import { themeDeckoalaDark } from './theme-deckoala-dark';
import { themeDeckoalaBold } from './theme-deckoala-bold';
import { getStyleContent, setBlock } from './frontmatter';
import { sanitizeCss } from './css-sanitize';

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
marp.themeSet.add(themeDeckoalaDark);
marp.themeSet.add(themeDeckoalaBold);

/** The themes a deck may choose from (id + human label + whether it is dark,
 * for the gallery thumbnail chrome). */
export const THEMES = [
	{ id: 'deckoala', dark: false },
	{ id: 'deckoala-dark', dark: true },
	{ id: 'deckoala-bold', dark: false }
] as const;

export type RenderedDeck = {
	html: string;
	css: string;
	slideCount: number;
	/** Speaker notes per slide (Marp HTML comments; directive comments like
	 * `<!-- _class: lead -->` are excluded by marp-core). '' when a slide has
	 * no notes. Rendered as plain text — never as HTML. */
	notes: string[];
};

/** Sanitize the deck's own custom CSS (the `style:` block) before Marp compiles
 * it. This is the SINGLE choke point: every caller of renderDeck (preview, slide
 * rail, present, print, share view, SharePresent, gallery thumbnails) is covered
 * by construction, so no viewer surface can issue an external request through
 * custom CSS (BRIEF-0009c). Only the user block is touched — Marp's own theme
 * CSS and KaTeX `@font-face url(/katex-fonts/…)` never pass through here. */
function sanitizeDeckStyle(markdown: string): string {
	// Covers BOTH the `style: |` block and inline `style: "…"` scalar forms —
	// an inline directive is a valid Marpit global style too.
	const style = getStyleContent(markdown);
	if (!style) return markdown;
	const cleaned = sanitizeCss(style.join('\n')).split('\n');
	// Always write back as a block: safe indentation, no YAML escaping worries.
	// Only this throwaway render copy is changed; the stored deck keeps its form.
	return setBlock(markdown, 'style', cleaned);
}

export function renderDeck(markdown: string): RenderedDeck {
	const { html, css, comments } = marp.render(sanitizeDeckStyle(markdown));
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
