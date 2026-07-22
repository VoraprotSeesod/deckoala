import { describe, it, expect } from 'vitest';
import { imageMarkdown, sanitizeAlt } from './slide-syntax';

describe('imageMarkdown', () => {
	const url = '/assets/d1/p.png';

	it('inline full-width has no width keyword', () => {
		expect(imageMarkdown({ url, alt: 'Photo', mode: 'inline', size: 'full' })).toBe('![Photo](/assets/d1/p.png)');
	});

	it('inline small/medium emit the right w: keyword after the alt', () => {
		expect(imageMarkdown({ url, alt: 'Photo', mode: 'inline', size: 'small' })).toBe('![Photo w:320](/assets/d1/p.png)');
		expect(imageMarkdown({ url, alt: 'Photo', mode: 'inline', size: 'medium' })).toBe('![Photo w:640](/assets/d1/p.png)');
	});

	it('background mode uses the bg keyword family and ignores size', () => {
		expect(imageMarkdown({ url, alt: 'x', mode: 'background', bgVariant: 'default' })).toBe('![bg](/assets/d1/p.png)');
		expect(imageMarkdown({ url, alt: 'x', mode: 'background', bgVariant: 'fit' })).toBe('![bg fit](/assets/d1/p.png)');
		expect(imageMarkdown({ url, alt: 'x', mode: 'background', bgVariant: 'left' })).toBe('![bg left](/assets/d1/p.png)');
	});

	it('sanitizes alt text that would break the ![]() syntax', () => {
		expect(imageMarkdown({ url, alt: 'a [b](c)', mode: 'inline', size: 'full' })).toBe('![a bc](/assets/d1/p.png)');
	});

	it('keeps Thai alt text', () => {
		expect(imageMarkdown({ url, alt: 'รูปภาพ', mode: 'inline', size: 'full' })).toBe('![รูปภาพ](/assets/d1/p.png)');
	});
});

describe('sanitizeAlt', () => {
	it('strips structural characters and falls back to "image"', () => {
		expect(sanitizeAlt('my[pic].png')).toBe('mypic.png');
		expect(sanitizeAlt('()')).toBe('image');
		expect(sanitizeAlt('  ')).toBe('image');
	});
});
