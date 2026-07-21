import { describe, it, expect } from 'vitest';
import {
	baseCommands,
	buildCommands,
	deckCommands,
	filterCommands,
	score,
	type Command,
	type CommandContext
} from './commands';

function ctx(over: Partial<CommandContext> = {}): CommandContext {
	return {
		route: 'dashboard',
		isAdmin: false,
		decks: [],
		navigate: () => {},
		formatUpdated: (iso) => `updated ${iso}`,
		pageCommands: [],
		openHelp: () => {},
		toggleTheme: () => {},
		toggleLocale: () => {},
		...over
	};
}

describe('score', () => {
	it('matches a subsequence and rejects a non-match', () => {
		expect(score('nd', 'new deck')).not.toBeNull();
		expect(score('zzz', 'new deck')).toBeNull();
	});

	it('is case insensitive', () => {
		expect(score('NEW', 'new deck')).not.toBeNull();
		expect(score('new', 'NEW DECK')).not.toBeNull();
	});

	it('ranks a prefix hit above a mid-word hit', () => {
		const prefix = score('de', 'decks')!;
		const middle = score('de', 'a wide deck')!;
		expect(prefix).toBeGreaterThan(middle);
	});

	it('ranks contiguous matches above scattered ones', () => {
		expect(score('deck', 'deck')!).toBeGreaterThan(score('deck', 'd e c k')!);
	});

	it('prefers the shorter target on a tie', () => {
		expect(score('deck', 'deck')!).toBeGreaterThan(score('deck', 'deck of many things')!);
	});

	it('handles Thai without ASCII assumptions', () => {
		// "สไลด์" — the palette must find Thai labels, since the UI defaults to Thai.
		expect(score('สไลด์', 'สไลด์ของฉัน')).not.toBeNull();
		expect(score('ฟอนต์', 'สไลด์ของฉัน')).toBeNull();
	});

	it('treats an empty query as a match so the default list renders', () => {
		expect(score('', 'anything')).toBe(0);
		expect(score('   ', 'anything')).toBe(0);
	});
});

describe('command registry', () => {
	it('hides admin-only commands from non-admins', () => {
		expect(baseCommands(ctx()).some((c) => c.id === 'nav.admin')).toBe(false);
		expect(baseCommands(ctx({ isAdmin: true })).some((c) => c.id === 'nav.admin')).toBe(true);
	});

	it('offers nothing destructive — a stray Enter must not delete a deck', () => {
		const all = buildCommands(
			ctx({ isAdmin: true, decks: [{ id: 'a', title: 'Deck', updatedAt: '2026-07-22T00:00:00Z' }] })
		);
		const ids = all.map((c) => c.id).join(' ');
		expect(ids).not.toMatch(/delete|remove|destroy/i);
	});

	it('gives every deck a distinguishing second line', () => {
		// Every "New deck" is titled "Untitled deck", so the title alone is useless.
		const decks = [
			{ id: '1', title: 'Untitled deck', updatedAt: '2026-07-22T10:00:00Z' },
			{ id: '2', title: 'Untitled deck', updatedAt: '2026-07-21T10:00:00Z' }
		];
		const rows = deckCommands(ctx({ decks }));
		expect(rows).toHaveLength(2);
		expect(rows[0].detail).not.toBe(rows[1].detail);
		expect(rows[0].id).not.toBe(rows[1].id);
	});

	it('puts page-contributed commands first', () => {
		const page: Command = {
			id: 'page.new',
			section: 'action',
			labelKey: 'cmd.newDeck',
			run: () => {}
		};
		expect(buildCommands(ctx({ pageCommands: [page] }))[0].id).toBe('page.new');
	});

	it('runs a command synchronously — activation-gated actions die behind an await', () => {
		let ran = false;
		const page: Command = {
			id: 'page.import',
			section: 'action',
			labelKey: 'cmd.import',
			run: () => {
				ran = true;
			}
		};
		buildCommands(ctx({ pageCommands: [page] }))[0].run();
		expect(ran).toBe(true);
	});
});

describe('filterCommands', () => {
	const resolve = (c: Command) => c.label ?? c.labelKey;
	const commands: Command[] = [
		{ id: 'a', section: 'navigate', labelKey: 'Decks', run: () => {} },
		{ id: 'b', section: 'navigate', labelKey: 'Fonts', run: () => {} },
		{ id: 'c', section: 'deck', labelKey: '', label: 'Quarterly review', run: () => {} }
	];

	it('returns everything for an empty query, in registry order', () => {
		expect(filterCommands(commands, '', resolve).map((c) => c.id)).toEqual(['a', 'b', 'c']);
	});

	it('drops non-matches and ranks the best first', () => {
		const found = filterCommands(commands, 'fo', resolve);
		expect(found.map((c) => c.id)).toEqual(['b']);
	});

	it('matches deck rows by their literal title', () => {
		expect(filterCommands(commands, 'quarterly', resolve).map((c) => c.id)).toEqual(['c']);
	});

	it('finds a page-contributed command by English alias (the UI defaults to Thai)', () => {
		// Regression: page commands originally shipped without `keywords`, so
		// searching "import" against the Thai label "นำเข้า .md" found nothing.
		const page: Command[] = [
			{ id: 'page.import', section: 'action', labelKey: 'นำเข้า .md', keywords: 'import markdown md upload', run: () => {} }
		];
		expect(filterCommands(page, 'import', resolve).map((c) => c.id)).toEqual(['page.import']);
		expect(filterCommands(page, 'นำเข้า', resolve).map((c) => c.id)).toEqual(['page.import']);
	});

	it('matches an English alias while the label is Thai', () => {
		const thai: Command[] = [
			{ id: 'th', section: 'navigate', labelKey: 'ฟอนต์', keywords: 'fonts typeface', run: () => {} }
		];
		expect(filterCommands(thai, 'fonts', resolve).map((c) => c.id)).toEqual(['th']);
	});

	it('keeps a growing list stable by id, so an in-flight fetch cannot move the selection', () => {
		// The palette selects by id; this proves an id present before the deck
		// fetch resolves is still present (and findable) afterwards.
		const before = filterCommands(commands, 'de', resolve);
		const withDecks = filterCommands(
			[...commands, { id: 'deck.x', section: 'deck', labelKey: '', label: 'Deckoala demo', run: () => {} }],
			'de',
			resolve
		);
		const selected = before[0].id;
		expect(withDecks.some((c) => c.id === selected)).toBe(true);
	});
});
