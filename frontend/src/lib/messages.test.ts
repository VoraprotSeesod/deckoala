import { describe, it, expect } from 'vitest';
import { translate, type Locale } from './messages';

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
