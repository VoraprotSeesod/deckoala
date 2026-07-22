<script lang="ts">
	import { wrapInline, toggleLinePrefix, insertLink, insertBlock, type Sel, type InlineMarker, type LineKind } from '$lib/md-format';
	import { BLOCK, type BlockKind } from '$lib/slide-blocks';
	import { t } from '$lib/i18n.svelte';

	type Props = {
		disabled: boolean;
		/** Run a transform: given the current selection, returns the new state. */
		apply: (fn: (s: Sel) => Sel) => void;
		/** Insert a block at the (clamped) caret. */
		insert: (fn: (text: string, pos: number) => Sel) => void;
		/** Open the existing image picker. */
		onImage: () => void;
	};
	let { disabled, apply, insert, onImage }: Props = $props();

	const inline = (marker: InlineMarker) => apply((s) => wrapInline(s, marker));
	const line = (kind: LineKind) => apply((s) => toggleLinePrefix(s, kind));
	const link = () => apply((s) => insertLink(s));
	const block = (kind: BlockKind) => insert((text, pos) => insertBlock(text, pos, BLOCK[kind]));
</script>

<div class="toolbar" role="toolbar" aria-label={t('toolbar.label')}>
	<div class="group">
		<button {disabled} onclick={() => inline('**')} title={t('toolbar.bold')} aria-label={t('toolbar.bold')}><b>B</b></button>
		<button {disabled} onclick={() => inline('*')} title={t('toolbar.italic')} aria-label={t('toolbar.italic')}><i>I</i></button>
		<button {disabled} onclick={() => inline('~~')} title={t('toolbar.strike')} aria-label={t('toolbar.strike')}><s>S</s></button>
		<button {disabled} onclick={() => inline('`')} title={t('toolbar.code')} aria-label={t('toolbar.code')}>{'</>'}</button>
	</div>
	<div class="group">
		<button {disabled} onclick={() => line('h1')} title={t('toolbar.h1')} aria-label={t('toolbar.h1')}>H1</button>
		<button {disabled} onclick={() => line('h2')} title={t('toolbar.h2')} aria-label={t('toolbar.h2')}>H2</button>
		<button {disabled} onclick={() => line('h3')} title={t('toolbar.h3')} aria-label={t('toolbar.h3')}>H3</button>
		<button {disabled} onclick={() => line('bullet')} title={t('toolbar.bullet')} aria-label={t('toolbar.bullet')}>•</button>
		<button {disabled} onclick={() => line('numbered')} title={t('toolbar.numbered')} aria-label={t('toolbar.numbered')}>1.</button>
		<button {disabled} onclick={() => line('quote')} title={t('toolbar.quote')} aria-label={t('toolbar.quote')}>❝</button>
		<button {disabled} onclick={link} title={t('toolbar.link')} aria-label={t('toolbar.link')}>🔗</button>
	</div>
	<div class="group">
		<button {disabled} onclick={onImage} title={t('toolbar.image')} aria-label={t('toolbar.image')}>🖼</button>
		<button {disabled} onclick={() => block('table')} title={t('toolbar.table')} aria-label={t('toolbar.table')}>▦</button>
		<button {disabled} onclick={() => block('code')} title={t('toolbar.codeBlock')} aria-label={t('toolbar.codeBlock')}>{'{ }'}</button>
		<button {disabled} onclick={() => block('math')} title={t('toolbar.math')} aria-label={t('toolbar.math')}>∑</button>
		<button {disabled} onclick={() => block('columns')} title={t('toolbar.columns')} aria-label={t('toolbar.columns')}>▥</button>
		<button {disabled} onclick={() => block('center')} title={t('toolbar.center')} aria-label={t('toolbar.center')}>≡</button>
		<button {disabled} onclick={() => block('slideBreak')} title={t('toolbar.slideBreak')} aria-label={t('toolbar.slideBreak')}>⎯</button>
		<button {disabled} onclick={() => block('note')} title={t('toolbar.note')} aria-label={t('toolbar.note')}>✎</button>
	</div>
</div>

<style>
	.toolbar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.35rem 0.5rem;
		border-bottom: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		background: var(--dk-surface);
		overflow-x: auto; /* the row scrolls, never the page, at narrow widths */
		flex: 0 0 auto;
	}

	.group {
		display: flex;
		gap: 0.15rem;
		flex: 0 0 auto;
	}

	.group + .group {
		padding-left: 0.5rem;
		border-left: 1px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	button {
		font: inherit;
		font-size: 0.85rem;
		min-width: 1.9rem;
		height: 1.9rem;
		padding: 0 0.4rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border: 1px solid transparent;
		border-radius: 0.4rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
		line-height: 1;
	}

	button:hover:not(:disabled) {
		background: color-mix(in srgb, var(--dk-ink) 10%, transparent);
	}

	button:disabled {
		opacity: 0.4;
		cursor: default;
	}

	i,
	b,
	s {
		font-style: normal;
		font-weight: 400;
	}

	b {
		font-weight: 700;
	}

	i {
		font-style: italic;
	}

	s {
		text-decoration: line-through;
	}
</style>
