import { Marp } from '@marp-team/marp-core';
// Document-level KaTeX font registration: Chromium ignores @font-face inside
// Shadow DOM styles, but families registered at document level ARE usable by
// shadow content. Vite rewrites the CSS's font URLs to bundled (same-origin)
// assets, so the zero-external-request invariant holds. The shadow-injected
// marp CSS still carries the structural .katex rules (and its own
// /katex-fonts/ @font-face for browsers that do honor shadow font-faces).
import 'katex/dist/katex.min.css';
import { themeDeckoala } from './theme-deckoala';

// One shared renderer for preview / present / print (ADR-0002: a single
// client-side pipeline keeps everything WYSIWYG).
// - html: false → raw HTML in markdown stays escaped (XSS defense; decks
//   get shared cross-user in BRIEF-0008). Locked by BRIEF-0003.
// - katexFontPath points at our own static assets — marp-core's default is
//   a CDN, which would violate the zero-external-request invariant.
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
};

export function renderDeck(markdown: string): RenderedDeck {
	const { html, css, comments } = marp.render(markdown);
	return { html, css, slideCount: comments.length };
}
