/** Marp image markdown builder for the image picker (BRIEF-0009d).
 *
 * Pure + testable. Inline images use size keywords (`![alt w:320](url)`);
 * background images use the `bg` keyword family (`![bg fit](url)`) and take no
 * width, because Marp's inline sizing and background syntax don't combine.
 */

export type ImageMode = 'inline' | 'background';
export type ImageSize = 'small' | 'medium' | 'full';
export type BgVariant = 'default' | 'fit' | 'left' | 'right';

/** Strip Markdown-structural characters from filename-derived alt text so it
 * can't break the `![…]()` syntax (mirrors the editor's existing altText). */
export function sanitizeAlt(name: string): string {
	return name.replace(/[[\]()\\\r\n]/g, '').trim() || 'image';
}

const WIDTHS: Record<ImageSize, string> = { small: 'w:320', medium: 'w:640', full: '' };

export function imageMarkdown(opts: {
	url: string;
	alt: string;
	mode: ImageMode;
	size?: ImageSize;
	bgVariant?: BgVariant;
}): string {
	if (opts.mode === 'background') {
		const v = opts.bgVariant && opts.bgVariant !== 'default' ? ` ${opts.bgVariant}` : '';
		return `![bg${v}](${opts.url})`;
	}
	const alt = sanitizeAlt(opts.alt);
	const w = WIDTHS[opts.size ?? 'full'];
	// Keywords sit in the alt slot AFTER the alt text; Marp reads `w:` as a
	// sizing directive and keeps the rest as alt.
	const label = w ? `${alt} ${w}` : alt;
	return `![${label}](${opts.url})`;
}
