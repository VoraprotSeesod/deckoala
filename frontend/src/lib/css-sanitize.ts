/** Sanitize user-authored per-deck custom CSS (the `style:` block) — BRIEF-0009c.
 *
 * DEFENSE-IN-DEPTH. The served pages already carry a strict CSP
 * (`default-src 'self'; img-src 'self' data: blob:`), so an external fetch from
 * CSS is blocked at the browser regardless. This function is the second layer:
 * it strips the constructs that would attempt such a fetch, so a viewer page
 * (share / present / PDF) stays zero-external even if the CSP were ever relaxed.
 *
 * It runs ONLY on the user's custom CSS — never on `renderDeck()`'s output,
 * which contains trusted KaTeX `@font-face url(/katex-fonts/…)` rules that must
 * survive or math breaks on every deck.
 *
 * Because the CSP already blocks the network request, correctness is proven by
 * UNIT TEST (input → output), not by a network check (which would pass even
 * against a no-op).
 */

/** A leading scheme (`http:`, `data:`, …) or a protocol-relative `//host`. */
const HAS_SCHEME = /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i;

function stripComments(css: string): string {
	// Collapse comments first so split-token tricks (`ur/**/l(`, `@im/**/port`)
	// fall apart before any matching, and remove CSS line-continuations
	// (backslash-newline) so a scheme can't be split across lines.
	return css.replace(/\\\r?\n/g, '').replace(/\/\*[\s\S]*?\*\//g, ' ');
}

/** Decode CSS escapes so `\68ttp:` can't slip a scheme past the scheme check. */
function decodeCssEscapes(s: string): string {
	return s
		.replace(/\\([0-9a-fA-F]{1,6})\s?/g, (_, hex) => {
			try {
				return String.fromCodePoint(parseInt(hex, 16));
			} catch {
				return '';
			}
		})
		.replace(/\\(.)/g, '$1');
}

/** True when a url()/string target would fetch from another origin. Same-origin
 * (`/assets/…`, `/katex-fonts/…`, relative paths), `data:`, `blob:` and
 * fragments are allowed; schemes and `//host` are not. */
function targetIsExternal(raw: string): boolean {
	let t = raw.trim();
	if (
		(t.startsWith('"') && t.endsWith('"')) ||
		(t.startsWith("'") && t.endsWith("'"))
	) {
		t = t.slice(1, -1);
	}
	t = decodeCssEscapes(t).trim();
	if (t === '' || t.startsWith('#')) return false;
	if (t.startsWith('data:') || t.startsWith('blob:')) return false;
	if (t.startsWith('//')) return true; // protocol-relative → external
	if (t.startsWith('/')) return false; // root-relative → same origin
	if (HAS_SCHEME.test(t)) return true; // http:, https:, ftp:, anything:
	return false; // relative path → same origin
}

const BLANK = 'url(about:blank)';

/** Neutralize every external `url( … )`, in any property or at-rule. */
function neutralizeUrls(css: string): string {
	return css.replace(/url\(\s*([^)]*)\)/gi, (whole, inner: string) =>
		targetIsExternal(inner) ? BLANK : whole
	);
}

export function sanitizeCss(css: string): string {
	let out = stripComments(css);

	// @import can pull in a remote stylesheet — drop it entirely (bounded to the
	// statement so it can't swallow later rules).
	out = out.replace(/@import[^;\n]*;?/gi, '');

	// image-set()/-webkit-image-set() fetch their candidates; clean external
	// string literals (its url() candidates are handled by neutralizeUrls).
	out = out.replace(/(-webkit-)?image-set\(([^)]*)\)/gi, (_m, prefix = '', inner: string) => {
		const cleaned = inner.replace(/(['"])([^'"]*)\1/g, (lit, q, val) =>
			targetIsExternal(val) ? `${q}about:blank${q}` : lit
		);
		return `${prefix}image-set(${cleaned})`;
	});

	out = neutralizeUrls(out);

	// Legacy IE / binding vectors — inert on modern engines, but strip anyway.
	out = out.replace(/expression\s*\(/gi, 'blocked(');
	out = out.replace(/javascript:/gi, 'blocked:');
	out = out.replace(/-moz-binding\s*:/gi, '-blocked-binding:');
	out = out.replace(/\bbehavior\s*:/gi, 'blocked-behavior:');

	return out;
}
