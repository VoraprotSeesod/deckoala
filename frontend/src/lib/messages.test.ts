import { describe, it, expect } from 'vitest';
import { translate, messages, type Locale } from './messages';

describe('catalog parity', () => {
	// translate() falls back TH→EN silently, so a key present in en but missing
	// in th would ship as English, not a visible raw key. Guard both directions
	// (BRIEF-0009c review).
	it('th and en have identical key sets', () => {
		const en = Object.keys(messages.en).sort();
		const th = Object.keys(messages.th).sort();
		const missingInTh = en.filter((k) => !(k in messages.th));
		const missingInEn = th.filter((k) => !(k in messages.en));
		expect(missingInTh, 'keys missing from th').toEqual([]);
		expect(missingInEn, 'keys missing from en').toEqual([]);
		expect(th).toEqual(en);
	});
});

describe('translate', () => {
	it('resolves a string in the requested locale', () => {
		expect(translate('en', 'dash.title')).toBe('Decks');
		expect(translate('th', 'dash.title')).toBe('สไลด์ของฉัน');
	});

	it('falls back to English when the locale is unknown', () => {
		// An out-of-catalog locale still yields the English string, never blank.
		expect(translate('xx' as Locale, 'dash.title')).toBe('Decks');
	});

	it('falls back to the key itself for an unknown key (never blank, never throws)', () => {
		expect(translate('th', 'totally.missing.key')).toBe('totally.missing.key');
		expect(translate('en', 'totally.missing.key')).toBe('totally.missing.key');
	});

	it('interpolates {name} params in string messages', () => {
		expect(translate('en', 'dash.updated', { when: 'just now' })).toBe('Updated just now');
	});

	it('leaves an unmatched {param} placeholder intact', () => {
		expect(translate('en', 'dash.updated', {})).toBe('Updated {when}');
	});

	it('handles function messages incl. pluralization', () => {
		expect(translate('en', 'editor.slides', { n: 1 })).toBe('1 slide');
		expect(translate('en', 'editor.slides', { n: 3 })).toBe('3 slides');
		// Thai has no plural inflection.
		expect(translate('th', 'editor.slides', { n: 3 })).toBe('3 สไลด์');
	});
});
