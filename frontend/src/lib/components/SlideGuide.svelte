<script lang="ts">
	import { tick } from 'svelte';
	import GuideBody from '$lib/components/GuideBody.svelte';
	import { t } from '$lib/i18n.svelte';

	type Props = {
		open: boolean;
		/** Drop a snippet at the editor cursor. */
		onInsert: (code: string) => void;
		onClose: () => void;
	};
	let { open, onInsert, onClose }: Props = $props();

	let panel = $state<HTMLDivElement | null>(null);
	let restoreTo: HTMLElement | null = null;

	$effect(() => {
		if (!open) return;
		restoreTo = document.activeElement as HTMLElement | null;
		const prev = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		tick().then(() => panel?.focus());
		return () => {
			document.body.style.overflow = prev;
			restoreTo?.focus?.();
			restoreTo = null;
		};
	});

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		}
	}

	function insert(code: string) {
		onInsert(code);
		onClose();
	}
</script>

{#if open}
	<div class="scrim">
		<button type="button" class="backdrop" aria-label={t('help.close')} onclick={onClose}></button>
		<div
			class="panel"
			role="dialog"
			aria-modal="true"
			aria-label={t('guide.title')}
			tabindex="-1"
			bind:this={panel}
			onkeydown={onKeydown}
		>
			<header>
				<h2>{t('guide.title')}</h2>
				<button type="button" onclick={onClose}>{t('help.close')}</button>
			</header>
			<div class="body">
				<GuideBody onInsert={insert} />
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
		background: rgba(11, 18, 21, 0.45);
		cursor: default;
	}

	.panel {
		position: relative;
		width: min(44rem, 100%);
		max-height: min(85svh, calc(100svh - 1.5rem));
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
		padding: 1rem;
	}
</style>
