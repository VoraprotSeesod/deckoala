<script lang="ts">
	import { tick } from 'svelte';
	import { SHORTCUTS, chordLabel, type Platform, type Scope } from '$lib/shortcuts';
	import { t } from '$lib/i18n.svelte';

	type Props = { open: boolean; platform: Platform; onClose: () => void };
	let { open, platform, onClose }: Props = $props();

	let closeBtn = $state<HTMLButtonElement | null>(null);
	let restoreTo: HTMLElement | null = null;

	const groups: { scope: Scope; titleKey: string }[] = [
		{ scope: 'app', titleKey: 'help.scopeApp' },
		{ scope: 'editor', titleKey: 'help.scopeEditor' },
		{ scope: 'present', titleKey: 'help.scopePresent' }
	];

	$effect(() => {
		if (!open) return;
		restoreTo = document.activeElement as HTMLElement | null;
		const previousOverflow = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		tick().then(() => closeBtn?.focus());
		return () => {
			document.body.style.overflow = previousOverflow;
			restoreTo?.focus?.();
			restoreTo = null;
		};
	});

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		} else if (event.key === 'Tab') {
			event.preventDefault();
		}
	}
</script>

{#if open}
	<div class="scrim">
		<button type="button" class="backdrop" aria-label={t('help.close')} onclick={onClose}></button>
		<div
			class="panel"
			role="dialog"
			aria-modal="true"
			aria-label={t('help.title')}
			tabindex="-1"
			onkeydown={onKeydown}
		>
			<header>
				<h2>{t('help.title')}</h2>
				<button bind:this={closeBtn} type="button" onclick={onClose}>{t('help.close')}</button>
			</header>
			<div class="body">
				{#each groups as group (group.scope)}
					<section>
						<h3>{t(group.titleKey)}</h3>
						<dl>
							{#each SHORTCUTS.filter((s) => s.scope === group.scope) as spec (spec.actionKey + spec.scope)}
								<div class="row">
									<dt>{t(spec.actionKey)}</dt>
									<dd>
										{#each spec.keys as chord, i (i)}
											{#if i > 0}<span class="or">/</span>{/if}
											<kbd>{chordLabel(chord, platform)}</kbd>
										{/each}
									</dd>
								</div>
							{/each}
						</dl>
						{#if group.scope === 'present'}
							<p class="note">{t('help.presentNote')}</p>
						{/if}
					</section>
				{/each}
			</div>
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
		align-items: flex-start;
		padding: max(0.75rem, env(safe-area-inset-top)) 0.75rem 0.75rem;
	}

	.backdrop {
		position: absolute;
		inset: 0;
		border: none;
		padding: 0;
		/* Fixed brand ink — --dk-ink inverts light in dark mode (see CommandPalette). */
		background: rgba(11, 18, 21, 0.45);
		cursor: default;
	}

	.panel {
		position: relative;
		width: min(34rem, 100%);
		max-height: min(80svh, calc(100svh - 1.5rem));
		display: flex;
		flex-direction: column;
		background: var(--dk-surface);
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 20%, transparent);
		border-radius: 0.85rem;
		box-shadow: 0 18px 50px rgba(11, 18, 21, 0.3);
		overflow: hidden;
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1rem;
		border-bottom: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	h2 {
		margin: 0;
		font-size: 1.05rem;
	}

	header button {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.3rem 0.7rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
	}

	.body {
		overflow-y: auto;
		padding: 0.5rem 1rem 1rem;
	}

	h3 {
		font-size: 0.8rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		opacity: 0.55;
		margin: 1rem 0 0.35rem;
	}

	dl {
		margin: 0;
	}

	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.3rem 0;
		border-bottom: 1px solid color-mix(in srgb, var(--dk-ink) 8%, transparent);
	}

	dt {
		font-size: 0.9rem;
		min-width: 0;
	}

	dd {
		margin: 0;
		display: flex;
		align-items: center;
		gap: 0.25rem;
		flex-shrink: 0;
	}

	.or {
		opacity: 0.4;
		font-size: 0.75rem;
	}

	kbd {
		font: inherit;
		font-size: 0.75rem;
		padding: 0.1rem 0.4rem;
		border: 1px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.3rem;
	}

	.note {
		font-size: 0.8rem;
		opacity: 0.6;
		margin: 0.5rem 0 0;
	}
</style>
