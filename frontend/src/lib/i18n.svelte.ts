// Reactive locale + theme, layered over the pure catalog in messages.ts.
// Browser globals are read only behind `browser` (adapter-static / vitest have
// no DOM). `$state` lives on a singleton object — a reassigned top-level
// `export let x = $state()` is a Svelte compile error.

import { browser } from '$app/environment';
import { DEFAULT_LOCALE, translate, type Locale, type MsgParams } from './messages';

export type Theme = 'light' | 'dark';

const LOCALE_KEY = 'deckoala-locale';
const THEME_KEY = 'deckoala-theme';
const TOOLBAR_KEY = 'deckoala-toolbar';

function storedLocale(): Locale {
	if (!browser) return DEFAULT_LOCALE;
	const v = localStorage.getItem(LOCALE_KEY);
	return v === 'en' || v === 'th' ? v : DEFAULT_LOCALE;
}

function storedTheme(): Theme {
	if (!browser) return 'light';
	const v = localStorage.getItem(THEME_KEY);
	if (v === 'light' || v === 'dark') return v;
	return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function storedToolbar(): boolean {
	if (!browser) return true;
	return localStorage.getItem(TOOLBAR_KEY) !== 'off'; // default on
}

class Settings {
	locale = $state<Locale>(storedLocale());
	theme = $state<Theme>(storedTheme());
	/** Editor formatting toolbar visibility (BRIEF-0012). */
	showToolbar = $state<boolean>(storedToolbar());
}

export const settings = new Settings();

export function toggleToolbar(): void {
	settings.showToolbar = !settings.showToolbar;
	if (browser) localStorage.setItem(TOOLBAR_KEY, settings.showToolbar ? 'on' : 'off');
}

/** Translate the key with the CURRENT locale (reactive: reading settings.locale
 * inside a template re-renders on toggle). */
export function t(key: string, params?: MsgParams): string {
	return translate(settings.locale, key, params);
}

export function setLocale(locale: Locale): void {
	settings.locale = locale;
	if (browser) localStorage.setItem(LOCALE_KEY, locale);
}

export function setTheme(theme: Theme): void {
	settings.theme = theme;
	if (browser) localStorage.setItem(THEME_KEY, theme);
}

export function toggleLocale(): void {
	setLocale(settings.locale === 'th' ? 'en' : 'th');
}

export function toggleTheme(): void {
	setTheme(settings.theme === 'dark' ? 'light' : 'dark');
}

function localeTag(locale: Locale): string {
	return locale === 'th' ? 'th-TH' : 'en-US';
}

/** Locale-aware date/time so timestamps follow the TH/EN toggle. */
export function formatDate(ts: string | number | Date): string {
	return new Date(ts).toLocaleString(localeTag(settings.locale));
}

export function formatTime(ts: string | number | Date): string {
	return new Date(ts).toLocaleTimeString(localeTag(settings.locale));
}
