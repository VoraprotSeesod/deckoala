import { describe, it, expect } from 'vitest';
import {
	SHORTCUTS,
	chordLabel,
	detectPlatform,
	isTypingTarget,
	keyLabel,
	match,
	type MatchContext
} from './shortcuts';

/** Minimal stand-in for a KeyboardEvent — vitest runs in jsdom, but building
 * these by hand keeps the matcher honest about what it actually reads. */
function key(
	k: string,
	opts: { meta?: boolean; ctrl?: boolean; shift?: boolean; alt?: boolean; target?: unknown } = {}
): KeyboardEvent {
	return {
		key: k,
		metaKey: !!opts.meta,
		ctrlKey: !!opts.ctrl,
		shiftKey: !!opts.shift,
		altKey: !!opts.alt,
		target: opts.target ?? { tagName: 'BODY', isContentEditable: false, closest: () => null }
	} as unknown as KeyboardEvent;
}

function ctx(over: Partial<MatchContext> = {}): MatchContext {
	return { platform: 'other', route: 'app', overlayOpen: false, ...over };
}

describe('platform', () => {
	it('detects macOS and everything else', () => {
		expect(detectPlatform('Mozilla/5.0 (Macintosh)', 'MacIntel')).toBe('mac');
		expect(detectPlatform('Mozilla/5.0 (iPhone)', '')).toBe('mac');
		expect(detectPlatform('Mozilla/5.0 (Windows NT 10.0)', 'Win32')).toBe('other');
	});

	it('never tells a Windows user to press Command', () => {
		expect(keyLabel('Mod', 'other')).toBe('Ctrl');
		expect(keyLabel('Mod', 'mac')).toBe('⌘');
		expect(chordLabel(['Mod', 'K'], 'other')).toBe('Ctrl+K');
		expect(chordLabel(['Mod', 'K'], 'mac')).toBe('⌘K');
	});
});

describe('match', () => {
	it('opens the palette on Mod-K per platform', () => {
		expect(match(key('k', { ctrl: true }), ctx())).toBe('palette');
		expect(match(key('k', { meta: true }), ctx({ platform: 'mac' }))).toBe('palette');
	});

	it('ignores the other platform modifier (Ctrl-K on a Mac is readline)', () => {
		expect(match(key('k', { ctrl: true }), ctx({ platform: 'mac' }))).toBeNull();
		expect(match(key('k', { meta: true }), ctx())).toBeNull();
	});

	it('opens the palette on Mod-Shift-P but never binds Mod-Shift-K', () => {
		expect(match(key('p', { ctrl: true, shift: true }), ctx())).toBe('palette');
		// CodeMirror binds Shift-Mod-k to deleteLine — we must not shadow it.
		expect(match(key('k', { ctrl: true, shift: true }), ctx())).toBeNull();
	});

	it('saves on Mod-S only in the editor', () => {
		expect(match(key('s', { ctrl: true }), ctx({ route: 'editor' }))).toBe('save');
		expect(match(key('s', { ctrl: true }), ctx({ route: 'dashboard' }))).toBeNull();
	});

	it('fires modified chords even while typing', () => {
		const typing = { tagName: 'TEXTAREA', isContentEditable: false, closest: () => null };
		expect(match(key('s', { ctrl: true, target: typing }), ctx({ route: 'editor' }))).toBe('save');
		expect(match(key('k', { ctrl: true, target: typing }), ctx())).toBe('palette');
	});

	it('never steals a bare key from someone typing', () => {
		const input = { tagName: 'INPUT', isContentEditable: false, closest: () => null };
		const cm = { tagName: 'DIV', isContentEditable: true, closest: (s: string) => (s === '.cm-editor' ? {} : null) };
		expect(match(key('n', { target: input }), ctx({ route: 'dashboard' }))).toBeNull();
		expect(match(key('?', { target: input }), ctx())).toBeNull();
		expect(match(key('n', { target: cm }), ctx({ route: 'dashboard' }))).toBeNull();
		expect(match(key('?', { target: cm }), ctx())).toBeNull();
	});

	it('creates a deck on n only from the dashboard', () => {
		expect(match(key('n'), ctx({ route: 'dashboard' }))).toBe('newDeck');
		expect(match(key('n'), ctx({ route: 'editor' }))).toBeNull();
	});

	it('opens help on ?', () => {
		expect(match(key('?'), ctx())).toBe('help');
		expect(match(key('?'), ctx({ route: 'other' }))).toBeNull();
	});

	it('goes inert while an overlay is open, except Escape', () => {
		const open = ctx({ overlayOpen: true, route: 'dashboard' });
		expect(match(key('Escape'), open)).toBe('close');
		expect(match(key('n'), open)).toBeNull();
		expect(match(key('?'), open)).toBeNull();
	});

	it('ignores Escape when nothing is open, so present mode keeps its own handler', () => {
		expect(match(key('Escape'), ctx())).toBeNull();
	});

	it('ignores Alt combinations (AltGr produces real characters)', () => {
		expect(match(key('k', { ctrl: true, alt: true }), ctx())).toBeNull();
	});

	it('ignores auto-repeat on bare keys — holding N must not spawn a burst of decks', () => {
		const held = { ...key('n'), repeat: true } as KeyboardEvent;
		expect(match(key('n'), ctx({ route: 'dashboard' }))).toBe('newDeck');
		expect(match(held, ctx({ route: 'dashboard' }))).toBeNull();
	});

	it('still matches Mod-S on the editor route when focus is NOT in CodeMirror', () => {
		// The CodeMirror keymap only covers the caret-in-editor case; the layout
		// must handle the rest or the browser Save-page dialog appears.
		const onTitleInput = { tagName: 'INPUT', isContentEditable: false, closest: () => null };
		expect(match(key('s', { ctrl: true, target: onTitleInput }), ctx({ route: 'editor' }))).toBe('save');
		expect(match(key('s', { ctrl: true }), ctx({ route: 'editor' }))).toBe('save');
	});
});

describe('isTypingTarget', () => {
	it('recognises form fields, contenteditable and CodeMirror', () => {
		expect(isTypingTarget({ tagName: 'INPUT', closest: () => null } as unknown as EventTarget)).toBe(true);
		expect(isTypingTarget({ tagName: 'DIV', isContentEditable: true, closest: () => null } as unknown as EventTarget)).toBe(true);
		expect(
			isTypingTarget({ tagName: 'SPAN', isContentEditable: false, closest: (s: string) => (s === '.cm-editor' ? {} : null) } as unknown as EventTarget)
		).toBe(true);
		expect(isTypingTarget({ tagName: 'BODY', isContentEditable: false, closest: () => null } as unknown as EventTarget)).toBe(false);
		expect(isTypingTarget(null)).toBe(false);
	});

	it('falls back to the focused element when the target is not an element', () => {
		// Shadow DOM retargets events (the preview and slide rail are shadow
		// roots), and programmatic events may target `window`. These tests run
		// in the node environment, so stub the one global the fallback reads.
		const original = (globalThis as { document?: unknown }).document;
		const setActive = (el: unknown) => {
			(globalThis as { document?: unknown }).document = { activeElement: el };
		};
		try {
			setActive({ tagName: 'INPUT', isContentEditable: false, closest: () => null });
			expect(isTypingTarget({} as EventTarget)).toBe(true);
			setActive({ tagName: 'BODY', isContentEditable: false, closest: () => null });
			expect(isTypingTarget({} as EventTarget)).toBe(false);
			setActive(null);
			expect(isTypingTarget({} as EventTarget)).toBe(false);
		} finally {
			if (original === undefined) delete (globalThis as { document?: unknown }).document;
			else (globalThis as { document?: unknown }).document = original;
		}
	});
});

describe('SHORTCUTS table', () => {
	it('documents present mode, which hosts no help sheet of its own', () => {
		expect(SHORTCUTS.some((s) => s.scope === 'present')).toBe(true);
	});

	it('covers every scope the help sheet renders', () => {
		const scopes = new Set(SHORTCUTS.map((s) => s.scope));
		expect([...scopes].sort()).toEqual(['app', 'editor', 'present']);
	});

	it('never advertises a chord the matcher does not implement', () => {
		// Every app/editor entry carrying an action must be reachable.
		for (const spec of SHORTCUTS.filter((s) => s.scope !== 'present' && s.action)) {
			expect(spec.keys.length).toBeGreaterThan(0);
			expect(spec.actionKey.startsWith('sc.')).toBe(true);
		}
	});
});
