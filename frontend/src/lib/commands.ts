/** Command registry + fuzzy scorer for the palette (BRIEF-0009b).
 *
 * Pure module: imports nothing from `$app/*` and holds no reactive state, so
 * vitest imports it by relative path. Everything environment-specific —
 * navigation, the deck list, page-local handlers — arrives through `ctx`.
 */

export type CommandSection = 'deck' | 'action' | 'navigate' | 'settings';

export type Command = {
	id: string;
	section: CommandSection;
	/** Message key; resolved at render so the language toggle re-labels it. */
	labelKey: string;
	/** Literal label used instead of labelKey (deck titles). */
	label?: string;
	/** Dimmed second line. Never scored. */
	detail?: string;
	/** Extra text folded into matching, e.g. an English alias while the UI is Thai. */
	keywords?: string;
	/** Chord tokens for the trailing hint, e.g. ['Mod','K']. */
	shortcut?: string[];
	/** Invoked SYNCHRONOUSLY from the click/Enter handler — a file picker and
	 * anything else gated on transient user activation dies behind an await. */
	run: () => void;
};

export type DeckLike = { id: string; title: string; updatedAt: string };

export type CommandContext = {
	route: 'dashboard' | 'editor' | 'app';
	isAdmin: boolean;
	/** Decks for the jump list, newest first. */
	decks: DeckLike[];
	navigate: (href: string) => void;
	/** Formats a deck's updatedAt for the dimmed second line. */
	formatUpdated: (iso: string) => string;
	/** Commands contributed by the page that owns their state. */
	pageCommands: Command[];
	openHelp: () => void;
	toggleTheme: () => void;
	toggleLocale: () => void;
};

/** Navigation + settings commands, available on every /app page.
 * Deliberately contains nothing destructive: a stray Enter on a highlighted
 * row must never be able to delete a deck (BRIEF-0009b review finding 1). */
export function baseCommands(ctx: CommandContext): Command[] {
	const list: Command[] = [
		{
			id: 'nav.decks',
			section: 'navigate',
			labelKey: 'cmd.decks',
			keywords: 'decks dashboard home slides',
			run: () => ctx.navigate('/app')
		},
		{
			id: 'nav.fonts',
			section: 'navigate',
			labelKey: 'cmd.fonts',
			keywords: 'fonts typeface',
			run: () => ctx.navigate('/app/fonts')
		},
		{
			id: 'nav.tokens',
			section: 'navigate',
			labelKey: 'cmd.tokens',
			keywords: 'api token mcp',
			run: () => ctx.navigate('/app/tokens')
		},
		{
			id: 'settings.theme',
			section: 'settings',
			labelKey: 'cmd.toggleTheme',
			keywords: 'dark light theme',
			run: ctx.toggleTheme
		},
		{
			id: 'settings.locale',
			section: 'settings',
			labelKey: 'cmd.toggleLocale',
			keywords: 'language thai english ภาษา',
			run: ctx.toggleLocale
		},
		{
			id: 'help.shortcuts',
			section: 'settings',
			labelKey: 'cmd.help',
			keywords: 'keyboard shortcuts help',
			shortcut: ['?'],
			run: ctx.openHelp
		}
	];
	if (ctx.isAdmin) {
		list.splice(3, 0, {
			id: 'nav.admin',
			section: 'navigate',
			labelKey: 'cmd.admin',
			keywords: 'admin settings ai',
			run: () => ctx.navigate('/app/admin')
		});
	}
	return list;
}

/** Deck jump rows: two lines, newest first. Without the second line a real
 * user sees a wall of identical "Untitled deck" rows. */
export function deckCommands(ctx: CommandContext): Command[] {
	return ctx.decks.map((deck) => ({
		id: `deck.${deck.id}`,
		section: 'deck' as const,
		labelKey: '',
		label: deck.title,
		detail: ctx.formatUpdated(deck.updatedAt),
		run: () => ctx.navigate(`/app/deck/${deck.id}`)
	}));
}

export function buildCommands(ctx: CommandContext): Command[] {
	return [...ctx.pageCommands, ...baseCommands(ctx), ...deckCommands(ctx)];
}

// --- matching ---------------------------------------------------------------

/** Fold case without assuming ASCII — labels are Thai as often as English. */
function fold(text: string): string {
	return text.toLocaleLowerCase();
}

/**
 * Subsequence score, or null when `query` is not a subsequence of `text`.
 * Higher is better: contiguous runs and word-start hits rank up. Iterates
 * code points via the string iterator, never charCodeAt, so Thai (and any
 * astral character) behaves.
 */
export function score(query: string, text: string): number | null {
	const q = [...fold(query.trim())].filter((c) => c !== ' ');
	if (q.length === 0) return 0;
	const t = [...fold(text)];
	let qi = 0;
	let points = 0;
	let run = 0;
	for (let ti = 0; ti < t.length && qi < q.length; ti++) {
		if (t[ti] === q[qi]) {
			run += 1;
			points += run * 2; // contiguity bonus
			if (ti === 0) points += 8; // prefix hit
			else if (t[ti - 1] === ' ' || t[ti - 1] === '-') points += 4; // word start
			qi += 1;
		} else {
			run = 0;
		}
	}
	if (qi < q.length) return null;
	// Shorter targets win ties: a 4-char label beating a 40-char one is what a
	// user expects when both contain the query.
	return points - t.length / 100;
}

export type ScoredCommand = Command & { rank: number };

/** Filter + rank. Section order is preserved for equal ranks so an empty query
 * yields a stable, meaningful default list. */
export function filterCommands(commands: Command[], query: string, resolve: (c: Command) => string): ScoredCommand[] {
	const scored: ScoredCommand[] = [];
	for (const command of commands) {
		const haystack = `${resolve(command)} ${command.keywords ?? ''}`;
		const rank = score(query, haystack);
		if (rank !== null) scored.push({ ...command, rank });
	}
	if (!query.trim()) return scored;
	return scored.sort((a, b) => b.rank - a.rank);
}
