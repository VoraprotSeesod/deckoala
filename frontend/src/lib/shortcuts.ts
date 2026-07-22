/** Keyboard shortcuts (BRIEF-0009b).
 *
 * Pure module — no runes, no `$app/*` imports — so vitest can import it by
 * relative path the way `messages.test.ts` does. The component layer decides
 * what an action id means; this file only answers "which action, if any?".
 */

export type Platform = 'mac' | 'other';

/** Where a binding applies. Also the grouping used by the help sheet. */
export type Scope = 'app' | 'editor' | 'present';

export type ActionId =
	| 'palette'
	| 'help'
	| 'save'
	| 'newDeck'
	| 'close'
	| 'presentNext'
	| 'presentPrev'
	| 'presentFirst'
	| 'presentLast'
	| 'presentFullscreen';

export type ShortcutSpec = {
	scope: Scope;
	/** Message key for the human description. */
	actionKey: string;
	/** Key tokens, already platform-agnostic: 'Mod' resolves per platform. */
	keys: string[][];
	action?: ActionId;
};

/** The single source of truth for both the bindings and the help sheet.
 * Present-mode entries are documentation only — those keys are handled by
 * `present/[id]/+page.svelte`, which predates this brief and keeps its own
 * handler; listing them here is what makes the help sheet honest. */
export const SHORTCUTS: ShortcutSpec[] = [
	{ scope: 'app', actionKey: 'sc.palette', keys: [['Mod', 'K'], ['Mod', 'Shift', 'P']], action: 'palette' },
	{ scope: 'app', actionKey: 'sc.help', keys: [['?']], action: 'help' },
	{ scope: 'app', actionKey: 'sc.newDeck', keys: [['N']], action: 'newDeck' },
	{ scope: 'app', actionKey: 'sc.close', keys: [['Esc']], action: 'close' },
	{ scope: 'editor', actionKey: 'sc.save', keys: [['Mod', 'S']], action: 'save' },
	{ scope: 'present', actionKey: 'sc.presentNext', keys: [['→'], ['Space'], ['PageDown'], ['N']] },
	{ scope: 'present', actionKey: 'sc.presentPrev', keys: [['←'], ['PageUp'], ['P']] },
	{ scope: 'present', actionKey: 'sc.presentFirst', keys: [['Home']] },
	{ scope: 'present', actionKey: 'sc.presentLast', keys: [['End']] },
	{ scope: 'present', actionKey: 'sc.presentFullscreen', keys: [['F']] },
	{ scope: 'present', actionKey: 'sc.close', keys: [['Esc']] }
];

export function detectPlatform(userAgent: string, platform: string): Platform {
	return /mac|iphone|ipad|ipod/i.test(`${platform} ${userAgent}`) ? 'mac' : 'other';
}

/** Render a key token for display, e.g. Mod → ⌘ on macOS, Ctrl elsewhere. */
export function keyLabel(token: string, platform: Platform): string {
	if (token === 'Mod') return platform === 'mac' ? '⌘' : 'Ctrl';
	if (token === 'Shift') return platform === 'mac' ? '⇧' : 'Shift';
	if (token === 'Esc') return platform === 'mac' ? '⎋' : 'Esc';
	return token;
}

/** A whole chord, e.g. ['Mod','K'] → "⌘K" or "Ctrl+K". */
export function chordLabel(keys: string[], platform: Platform): string {
	const parts = keys.map((k) => keyLabel(k, platform));
	return platform === 'mac' ? parts.join('') : parts.join('+');
}

/** Elements that own their keystrokes. An unmodified shortcut must never
 * steal a character the user is typing. */
export function isTypingTarget(target: EventTarget | null): boolean {
	let el = target as HTMLElement | null;
	// An event retargeted by a shadow root (this app renders the preview and the
	// slide rail into shadow DOM) or dispatched programmatically may not carry a
	// usable element; the focused element is the honest answer then.
	if (!el || typeof el.tagName !== 'string') {
		el = typeof document !== 'undefined' ? (document.activeElement as HTMLElement | null) : null;
	}
	if (!el || typeof el.tagName !== 'string') return false;
	if (['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)) return true;
	if (el.isContentEditable) return true;
	// CodeMirror's editable surface is a contenteditable div; `closest` also
	// catches clicks landing on inner spans.
	return typeof el.closest === 'function' && !!el.closest('.cm-editor');
}

/** True when the platform's "command" modifier is held (⌘ on mac, Ctrl else). */
function hasMod(event: KeyboardEvent, platform: Platform): boolean {
	return platform === 'mac' ? event.metaKey : event.ctrlKey;
}

export type MatchContext = {
	platform: Platform;
	/** Route family the event happened on. */
	route: 'dashboard' | 'editor' | 'app' | 'other';
	/** An overlay is open: everything but Escape is inert. */
	overlayOpen: boolean;
};

/**
 * Map a keyboard event to an action, or null. Modified chords fire even while
 * typing (that is the point of ⌘S); bare keys never do.
 */
export function match(event: KeyboardEvent, ctx: MatchContext): ActionId | null {
	if (event.altKey) return null;
	const key = event.key;

	if (key === 'Escape') return ctx.overlayOpen ? 'close' : null;

	// While ANY overlay is open, only Escape does anything — including modified
	// chords. Otherwise Mod-K would open the palette on top of the theme/CSS/AI
	// modal, and Mod-S would save the deck from behind the custom-CSS editor.
	if (ctx.overlayOpen) return null;

	const mod = hasMod(event, ctx.platform);
	if (mod) {
		// Ignore the OTHER platform's modifier so Ctrl+K on a Mac (a real
		// readline binding) is not hijacked.
		if (ctx.platform === 'mac' ? event.ctrlKey : event.metaKey) return null;
		const lower = key.toLowerCase();
		if (lower === 'k' && !event.shiftKey && ctx.route !== 'other') return 'palette';
		if (lower === 'p' && event.shiftKey && ctx.route !== 'other') return 'palette';
		if (lower === 's' && !event.shiftKey && ctx.route === 'editor') return 'save';
		return null;
	}

	if (isTypingTarget(event.target)) return null;
	// Held keys must not machine-gun an action: holding `n` would otherwise
	// create a deck per repeat event.
	if (event.repeat) return null;
	if (key === '?') return ctx.route !== 'other' ? 'help' : null;
	if (key.toLowerCase() === 'n') return ctx.route === 'dashboard' ? 'newDeck' : null;
	return null;
}
