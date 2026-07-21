<script lang="ts">
	import { tick } from 'svelte';
	import { filterCommands, type Command, type ScoredCommand } from '$lib/commands';
	import { chordLabel, type Platform } from '$lib/shortcuts';
	import { t } from '$lib/i18n.svelte';

	type Props = {
		open: boolean;
		commands: Command[];
		platform: Platform;
		/** Set while the first decks.list() is in flight — Enter stays inert. */
		loadingDecks?: boolean;
		notice?: string;
		onClose: () => void;
	};

	let { open = $bindable(), commands, platform, loadingDecks = false, notice = '', onClose }: Props = $props();

	let query = $state('');
	/** Selection is tracked by command id, never by index: the deck fetch can
	 * grow the list between a keystroke and Enter, and an index would then
	 * point at a row the user never highlighted. */
	let activeId = $state('');
	let input = $state<HTMLInputElement | null>(null);
	let listEl = $state<HTMLUListElement | null>(null);
	let restoreTo: HTMLElement | null = null;

	function label(command: Command): string {
		return command.label ?? t(command.labelKey);
	}

	const results = $derived(filterCommands(commands, query, label));
	const activeIndex = $derived(Math.max(0, results.findIndex((r) => r.id === activeId)));
	const active = $derived<ScoredCommand | undefined>(results[activeIndex]);

	const sectionLabels: Record<string, string> = $derived({
		deck: t('palette.sectionDeck'),
		action: t('palette.sectionAction'),
		navigate: t('palette.sectionNavigate'),
		settings: t('palette.sectionSettings')
	});

	// Keep the selection valid as the list changes, preferring to KEEP the
	// current id (re-resolve) over resetting to row 0.
	$effect(() => {
		if (!open) return;
		if (results.length === 0) {
			activeId = '';
		} else if (!results.some((r) => r.id === activeId)) {
			activeId = results[0].id;
		}
	});

	$effect(() => {
		if (!open) return;
		restoreTo = document.activeElement as HTMLElement | null;
		query = '';
		// The list is the only scroller; stop the page behind it from moving.
		const previousOverflow = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		tick().then(() => input?.focus());
		return () => {
			document.body.style.overflow = previousOverflow;
			restoreTo?.focus?.();
			restoreTo = null;
		};
	});

	function moveTo(index: number) {
		if (results.length === 0) return;
		const clamped = Math.min(results.length - 1, Math.max(0, index));
		activeId = results[clamped].id;
		tick().then(() => {
			listEl?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
		});
	}

	/** MUST stay synchronous: a file picker and anything else gated on
	 * transient user activation dies if an await runs first. */
	function runCommand(command: ScoredCommand | undefined) {
		if (!command || loadingDecks) return;
		command.run();
		onClose();
	}

	function onKeydown(event: KeyboardEvent) {
		switch (event.key) {
			case 'ArrowDown':
				event.preventDefault();
				moveTo(activeIndex + 1);
				break;
			case 'ArrowUp':
				event.preventDefault();
				moveTo(activeIndex - 1);
				break;
			case 'Home':
				event.preventDefault();
				moveTo(0);
				break;
			case 'End':
				event.preventDefault();
				moveTo(results.length - 1);
				break;
			case 'Enter':
				event.preventDefault();
				runCommand(active);
				break;
			case 'Escape':
				event.preventDefault();
				onClose();
				break;
			case 'Tab':
				// Focus stays in the dialog: the input is the only tab stop.
				event.preventDefault();
				break;
		}
	}
</script>

{#if open}
	<div class="scrim">
		<!-- A plain button as the backdrop keeps svelte-check's a11y rules happy
		     (a click handler on a div needs a keyboard handler and a role). -->
		<button type="button" class="backdrop" aria-label={t('help.close')} onclick={onClose}></button>
		<div class="panel" role="dialog" aria-modal="true" aria-label={t('palette.open')}>
			<!-- svelte-ignore a11y_autofocus -->
			<input
				bind:this={input}
				bind:value={query}
				type="text"
				role="combobox"
				aria-expanded="true"
				aria-controls="palette-list"
				aria-activedescendant={active ? `palette-opt-${active.id}` : undefined}
				aria-autocomplete="list"
				autocomplete="off"
				spellcheck="false"
				placeholder={t('palette.placeholder')}
				onkeydown={onKeydown}
			/>

			{#if notice}<p class="notice" role="alert">{notice}</p>{/if}
			{#if loadingDecks}<p class="notice subtle">{t('palette.loadingDecks')}</p>{/if}

			<ul bind:this={listEl} id="palette-list" role="listbox" aria-label={t('palette.open')}>
				{#each results as command (command.id)}
					<li
						id="palette-opt-{command.id}"
						role="option"
						aria-selected={command.id === activeId}
						class:active={command.id === activeId}
					>
						<button type="button" onclick={() => runCommand(command)} onmousemove={() => (activeId = command.id)}>
							<span class="text">
								<span class="label">{label(command)}</span>
								{#if command.detail}<span class="detail">{command.detail}</span>{/if}
							</span>
							<span class="right">
								<span class="section">{sectionLabels[command.section]}</span>
								{#if command.shortcut}<kbd>{chordLabel(command.shortcut, platform)}</kbd>{/if}
							</span>
						</button>
					</li>
				{:else}
					<li class="empty" role="option" aria-selected="false">{t('palette.noResults')}</li>
				{/each}
			</ul>
		</div>
	</div>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 60;
		display: flex;
		justify-content: center;
		/* Top-anchored, never vertically centred: a centred dialog that focuses
		   a text input ends up behind the phone's virtual keyboard. */
		align-items: flex-start;
		padding: max(0.75rem, env(safe-area-inset-top)) 0.75rem 0.75rem;
	}

	.backdrop {
		position: absolute;
		inset: 0;
		border: none;
		padding: 0;
		/* Fixed brand ink, NOT color-mix(--dk-ink): --dk-ink inverts to a light
		   colour in dark mode, which would wash the page out instead of dimming
		   it. Matches ShareManager/the AI dialog. */
		background: rgba(11, 18, 21, 0.45);
		cursor: default;
	}

	.panel {
		position: relative;
		width: min(40rem, 100%);
		max-height: min(70svh, calc(100svh - 1.5rem));
		display: flex;
		flex-direction: column;
		background: var(--dk-surface);
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 20%, transparent);
		border-radius: 0.85rem;
		box-shadow: 0 18px 50px rgba(11, 18, 21, 0.3);
		overflow: hidden;
	}

	input {
		font: inherit;
		font-size: 1rem;
		padding: 0.85rem 1rem;
		border: none;
		border-bottom: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		background: transparent;
		color: inherit;
	}

	input:focus {
		outline: none;
	}

	.notice {
		margin: 0;
		padding: 0.5rem 1rem;
		font-size: 0.85rem;
		color: var(--dk-danger);
		border-bottom: 1.5px solid color-mix(in srgb, var(--dk-ink) 8%, transparent);
	}

	.notice.subtle {
		color: inherit;
		opacity: 0.7;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0.35rem;
		overflow-y: auto;
		flex: 1;
	}

	li button {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		width: 100%;
		font: inherit;
		text-align: left;
		padding: 0.5rem 0.65rem;
		border: none;
		border-radius: 0.5rem;
		background: transparent;
		color: inherit;
		cursor: pointer;
	}

	li.active button {
		background: color-mix(in srgb, var(--dk-ink) 10%, transparent);
	}

	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.label {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.detail {
		font-size: 0.78rem;
		opacity: 0.6;
	}

	.right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}

	.section {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		opacity: 0.45;
	}

	kbd {
		font: inherit;
		font-size: 0.72rem;
		padding: 0.1rem 0.35rem;
		border: 1px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.3rem;
		opacity: 0.8;
	}

	.empty {
		padding: 0.9rem 0.65rem;
		opacity: 0.6;
		font-size: 0.9rem;
	}
</style>
