import { describe, it, expect } from 'vitest';
import { sanitizeCss } from './css-sanitize';

/** No functional external url() (or scheme-bearing url token) survives. */
function hasExternalFetch(css: string): boolean {
	// A url() whose target begins with a scheme or // — the only thing that fetches.
	return /url\(\s*['"]?\s*(?:[a-z][a-z0-9+.-]*:\/\/|\/\/)/i.test(css);
}

describe('sanitizeCss — blocks external fetches', () => {
	it('neutralizes http/https/protocol-relative url()', () => {
		for (const u of ['http://evil/x.png', 'https://evil/x.png', '//evil/x.png', 'HTTP://EVIL/X']) {
			const out = sanitizeCss(`section { background: url(${u}); }`);
			expect(hasExternalFetch(out)).toBe(false);
			expect(out).toContain('about:blank');
		}
	});

	it('sees through CSS comments splitting the token', () => {
		const out = sanitizeCss('section { background: ur/**/l(http://evil/x); }');
		expect(hasExternalFetch(out)).toBe(false);
	});

	it('sees through CSS escapes in the scheme', () => {
		// \68 == 'h'
		const out = sanitizeCss('section { background: url(\\68ttp://evil/x); }');
		expect(hasExternalFetch(out)).toBe(false);
	});

	it('handles quoted and unquoted, mixed case', () => {
		expect(hasExternalFetch(sanitizeCss('a{background:URL("http://e/x")}'))).toBe(false);
		expect(hasExternalFetch(sanitizeCss("a{background:url('http://e/x')}"))).toBe(false);
		expect(hasExternalFetch(sanitizeCss('a{background:url( http://e/x )}'))).toBe(false);
	});

	it('drops @import (url and string forms)', () => {
		expect(sanitizeCss('@import url(http://evil/x.css); a{}')).not.toMatch(/@import/i);
		expect(sanitizeCss('@import "http://evil/x.css"; a{}')).not.toMatch(/@import/i);
	});

	it('cleans cursor:url and @font-face src:url', () => {
		expect(hasExternalFetch(sanitizeCss('a{cursor:url(http://e/c.cur),auto}'))).toBe(false);
		expect(hasExternalFetch(sanitizeCss('@font-face{font-family:x;src:url(https://e/f.woff2)}'))).toBe(false);
	});

	it('cleans external candidates in image-set()', () => {
		const out = sanitizeCss("a{background-image:image-set('http://e/x.png' 1x, url(https://e/y.png) 2x)}");
		expect(hasExternalFetch(out)).toBe(false);
		expect(out).not.toMatch(/['"]https?:/i);
	});

	it('strips expression/javascript/binding vectors', () => {
		const out = sanitizeCss('a{width:expression(alert(1));background:url(javascript:alert(1));-moz-binding:url(x)}');
		expect(out).not.toMatch(/expression\s*\(/i);
		expect(out).not.toMatch(/javascript:/i);
		expect(out).not.toMatch(/-moz-binding\s*:/i);
	});
});

describe('sanitizeCss — preserves legitimate same-origin CSS', () => {
	it('keeps /assets, /katex-fonts, relative, data: and blob: urls', () => {
		for (const u of [
			'/assets/deck1/photo.png',
			'/katex-fonts/KaTeX_Main.woff2',
			'photo.png',
			'data:image/png;base64,AAAA',
			'blob:abc'
		]) {
			const css = `section { background: url(${u}); }`;
			const out = sanitizeCss(css);
			expect(out).toContain(u);
			expect(out).not.toContain('about:blank');
		}
	});

	it('leaves ordinary declarations untouched', () => {
		const css = 'h1 { color: #ff0000; font-size: 2em; }\nsection { padding: 40px; }';
		expect(sanitizeCss(css)).toBe(css);
	});

	it('does not corrupt a fragment url (SVG filter reference)', () => {
		const css = 'section { filter: url(#blur); }';
		expect(sanitizeCss(css)).toBe(css);
	});

	it('is idempotent', () => {
		const css = 'a{background:url(http://e/x)} b{color:red}';
		const once = sanitizeCss(css);
		expect(sanitizeCss(once)).toBe(once);
	});
});
