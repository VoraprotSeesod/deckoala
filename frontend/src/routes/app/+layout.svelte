<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError, type DeckMeta } from '$lib/api';
	import { buildCommands, type Command } from '$lib/commands';
	import { detectPlatform, match, type Platform } from '$lib/shortcuts';
	import { setPalette } from '$lib/palette.svelte';
	import { formatDate, t, toggleLocale, toggleTheme } from '$lib/i18n.svelte';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import SettingsToggle from '$lib/components/SettingsToggle.svelte';
	import ShortcutHelp from '$lib/components/ShortcutHelp.svelte';

	let { data, children } = $props();
	let logoutError = $state('');

	let paletteOpen = $state(false);
	let helpOpen = $state(false);
	let decks = $state<DeckMeta[]>([]);
	let decksLoaded = $state(false);
	let loadingDecks = $state(false);
	let paletteNotice = $state('');
	let pageCommands = $state<Command[]>([]);
	let platform = $state<Platform>('other');

	$effect(() => {
		platform = detectPlatform(navigator.userAgent, navigator.platform ?? '');
	});

	const route = $derived.by((): 'dashboard' | 'editor' | 'app' => {
		const id = page.route.id ?? '';
		if (id === '/app') return 'dashboard';
		if (id.startsWith('/app/deck/')) return 'editor';
		return 'app';
	});

	/** Fetch once, then reuse. The /app layout load returns only `user`, and
	 * loading decks on every /app page would tax /app/fonts, /tokens, /admin
	 * for nothing. */
	async function loadDecks() {
		if (decksLoaded || loadingDecks) return;
		loadingDecks = true;
		try {
			decks = await api.decks.list();
			decksLoaded = true;
			paletteNotice = '';
		} catch (e) {
			// A dead session is not a degraded palette — follow the app-wide rule.
			if (e instanceof ApiError && (e.status === 401 || e.status === 403)) {
				goto('/login');
				return;
			}
			paletteNotice = t('palette.decksUnavailable');
		} finally {
			loadingDecks = false;
		}
	}

	function openPalette() {
		helpOpen = false;
		paletteOpen = true;
		loadDecks();
	}

	setPalette({
		register: (commands: Command[]) => {
			pageCommands = commands;
			return () => {
				pageCommands = [];
			};
		},
		open: openPalette,
		invalidateDecks: () => {
			decksLoaded = false;
			decks = [];
		}
	});

	const commands = $derived(
		buildCommands({
			route,
			isAdmin: data.user.isAdmin,
			decks,
			navigate: (href) => {
				// `goto` is async, but the command has already returned by then —
				// the synchronous part (user activation) is preserved.
				goto(href)
					.then(() => {
						// A deck deleted in another tab still has a cached row here.
						// SvelteKit renders /app/+error.svelte for its 404 (the app
						// shell, header and palette all survive), and dropping the
						// cache means the dead row is gone the next time the palette
						// opens instead of reproducing the failure.
						if (href.startsWith('/app/deck/')) {
							decksLoaded = false;
							decks = [];
						}
					})
					.catch(() => {
						paletteNotice = t('palette.navFailed');
					});
			},
			formatUpdated: (iso) => t('dash.updated', { when: formatDate(iso) }),
			pageCommands,
			openHelp: () => {
				paletteOpen = false;
				helpOpen = true;
			},
			toggleTheme,
			toggleLocale
		})
	);

	/** Any modal counts, not just this brief's two: the AI dialog and the share
	 * manager also render role="dialog", and a bare `n` firing behind one of
	 * them would create a deck the user never asked for. */
	function anyOverlayOpen(): boolean {
		return paletteOpen || helpOpen || !!document.querySelector('[role="dialog"]');
	}

	function runPageCommand(id: string, event: KeyboardEvent) {
		const command = pageCommands.find((c) => c.id === id);
		if (!command) return;
		event.preventDefault();
		command.run();
	}

	function onWindowKeydown(event: KeyboardEvent) {
		const action = match(event, { platform, route, overlayOpen: anyOverlayOpen() });
		if (!action) return;
		switch (action) {
			case 'palette':
				event.preventDefault();
				openPalette();
				break;
			case 'help':
				event.preventDefault();
				paletteOpen = false;
				helpOpen = true;
				break;
			case 'newDeck':
				runPageCommand('page.newDeck', event);
				break;
			// Mod-S is matched for the whole editor route, not just inside
			// CodeMirror (whose own keymap handles the caret-in-editor case).
			// Without this arm the browser's Save-page dialog opens whenever
			// focus is on the title input, the slide rail, or plain <body>.
			case 'save':
				runPageCommand('page.save', event);
				break;
			case 'close':
				event.preventDefault();
				paletteOpen = false;
				helpOpen = false;
				break;
		}
	}

	async function logout() {
		logoutError = '';
		try {
			await api.logout();
			goto('/login');
		} catch {
			// Do NOT navigate: the session is still alive — pretending we
			// signed out would be worse than admitting the failure.
			logoutError = t('nav.logoutError');
		}
	}
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div class="shell">
	<header>
		<a class="brand" href="/app">
			<img src="/logo.svg" alt="" width="28" height="28" />
			<span>Deckoala</span>
		</a>
		<div class="session">
			{#if logoutError}<span class="logout-error" role="alert">{logoutError}</span>{/if}
			<a class="navlink" href="/app/research">{t('nav.research')}</a>
			<a class="navlink" href="/app/guide">{t('nav.guide')}</a>
			<a class="navlink" href="/app/fonts">{t('nav.fonts')}</a>
			<a class="navlink" href="/app/tokens">{t('nav.tokens')}</a>
			{#if data.user.isAdmin}
				<a class="navlink" href="/app/admin">{t('nav.admin')}</a>
			{/if}
			<span class="user">{data.user.username}</span>
			<!-- Icon-only on narrow viewports: a wrapping header would push the
			     editor's calc(100dvh - 8.5rem) article into a page scrollbar. -->
			<button class="palette-btn" onclick={openPalette} title={t('palette.open')} aria-label={t('palette.open')}>
				<span aria-hidden="true">⌘</span><span class="palette-label">{t('palette.open')}</span>
			</button>
			<SettingsToggle />
			<button onclick={logout}>{t('nav.logout')}</button>
		</div>
	</header>
	<main>
		{@render children()}
	</main>
</div>

<CommandPalette
	bind:open={paletteOpen}
	{commands}
	{platform}
	{loadingDecks}
	notice={paletteNotice}
	onClose={() => (paletteOpen = false)}
/>
<ShortcutHelp open={helpOpen} {platform} onClose={() => (helpOpen = false)} />

<style>
	.shell {
		min-height: 100dvh;
		display: flex;
		flex-direction: column;
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1rem;
		border-bottom: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	.brand {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-weight: 700;
		text-decoration: none;
		color: inherit;
	}

	.session {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-width: 0;
	}

	.user {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.navlink {
		font-weight: 600;
		font-size: 0.9rem;
		color: inherit;
		text-decoration: none;
	}

	.navlink:hover {
		text-decoration: underline;
	}

	.logout-error {
		color: var(--dk-danger);
		font-size: 0.85rem;
	}

	button {
		font: inherit;
		font-weight: 600;
		padding: 0.4rem 0.8rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
	}

	.palette-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.85rem;
		padding: 0.35rem 0.7rem;
		border-color: color-mix(in srgb, var(--dk-ink) 30%, transparent);
	}

	.palette-btn:hover {
		border-color: var(--dk-ink);
	}

	main {
		flex: 1;
		padding: 1.5rem 1rem;
	}

	@media (max-width: 720px) {
		.palette-label {
			display: none;
		}
	}
</style>
