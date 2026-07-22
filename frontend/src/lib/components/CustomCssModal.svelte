<script lang="ts">
	import { tick } from 'svelte';
	import { getBlock, setBlock, removeDirective } from '$lib/frontmatter';
	import { t } from '$lib/i18n.svelte';

	type Props = {
		open: boolean;
		markdown: string;
		onApply: (next: string) => void;
		onClose: () => void;
	};

	let { open, markdown, onApply, onClose }: Props = $props();

	// A plain textarea (not a second CodeMirror): no new dependency, no keymap
	// fight with the deck editor, and dark-mode-legible via the color-scheme fix.
	let value = $state('');
	let area = $state<HTMLTextAreaElement | null>(null);
	let panel = $state<HTMLDivElement | null>(null);
	let restoreTo: HTMLElement | null = null;

	$effect(() => {
		if (!open) return;
		value = (getBlock(markdown, 'style') ?? []).join('\n');
		restoreTo = document.activeElement as HTMLElement | null;
		const prev = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		tick().then(() => area?.focus());
		return () => {
			document.body.style.overflow = prev;
			restoreTo?.focus?.();
			restoreTo = null;
		};
	});

	function save() {
		const lines = value.split('\n');
		const nonEmpty = lines.some((l) => l.trim() !== '');
		onApply(nonEmpty ? setBlock(markdown, 'style', lines) : removeDirective(markdown, 'style'));
		onClose();
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		} else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
			// Mod-S saves the CSS here, not the deck.
			event.preventDefault();
			save();
		}
	}

	function onTextareaKeydown(event: KeyboardEvent) {
		// Tab inserts a tab in the textarea instead of leaving the field.
		if (event.key === 'Tab') {
			event.preventDefault();
			const el = event.currentTarget as HTMLTextAreaElement;
			const { selectionStart: s, selectionEnd: e } = el;
			value = value.slice(0, s) + '\t' + value.slice(e);
			tick().then(() => el.setSelectionRange(s + 1, s + 1));
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
			aria-label={t('css.title')}
			tabindex="-1"
			bind:this={panel}
			onkeydown={onKeydown}
		>
			<header>
				<h2>{t('css.title')}</h2>
				<button type="button" onclick={onClose}>{t('help.close')}</button>
			</header>
			<p class="hint">{t('css.hint')}</p>
			<textarea
				bind:this={area}
				bind:value
				spellcheck="false"
				autocomplete="off"
				placeholder={t('css.placeholder')}
				onkeydown={onTextareaKeydown}
			></textarea>
			<footer>
				<button type="button" class="ghost" onclick={onClose}>{t('css.cancel')}</button>
				<button type="button" class="primary" onclick={save}>{t('css.save')}</button>
			</footer>
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

	.hint {
		margin: 0;
		padding: 0.6rem 1rem 0;
		font-size: 0.85rem;
		opacity: 0.7;
	}

	textarea {
		flex: 1;
		min-height: 12rem;
		resize: vertical;
		margin: 0.6rem 1rem;
		padding: 0.7rem 0.85rem;
		font-family: ui-monospace, 'SFMono-Regular', Menlo, monospace;
		font-size: 0.85rem;
		line-height: 1.5;
		tab-size: 2;
		color: var(--dk-ink);
		background: color-mix(in srgb, var(--dk-ink) 5%, var(--dk-surface));
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.5rem;
	}

	footer {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		padding: 0.75rem 1rem;
		border-top: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	footer button {
		font: inherit;
		font-weight: 600;
		padding: 0.45rem 0.9rem;
		border-radius: 0.5rem;
		cursor: pointer;
	}

	.ghost {
		background: transparent;
		border: 1.5px solid var(--dk-ink);
		color: var(--dk-ink);
	}

	.primary {
		background: var(--dk-ink);
		border: 1.5px solid var(--dk-ink);
		color: var(--dk-bg);
	}
</style>
