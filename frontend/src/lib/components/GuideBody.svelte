<script lang="ts">
	import { GUIDE, type GuideSnippet } from '$lib/guide-content';
	import { t } from '$lib/i18n.svelte';

	type Props = {
		/** When provided, each snippet also offers "Insert at cursor". */
		onInsert?: (code: string) => void;
	};
	let { onInsert }: Props = $props();

	let copiedKey = $state('');

	async function copy(snippet: GuideSnippet) {
		try {
			await navigator.clipboard.writeText(snippet.code);
			copiedKey = snippet.labelKey;
			setTimeout(() => {
				if (copiedKey === snippet.labelKey) copiedKey = '';
			}, 1500);
		} catch {
			// Clipboard can be blocked; the code is on screen to select by hand.
		}
	}
</script>

<div class="guide">
	{#each GUIDE as section (section.titleKey)}
		<section>
			<h2>{t(section.titleKey)}</h2>
			<p class="intro">{t(section.introKey)}</p>
			{#each section.snippets as snippet (snippet.labelKey)}
				<figure>
					<figcaption>{t(snippet.labelKey)}</figcaption>
					<pre>{snippet.code}</pre>
					<div class="actions">
						<button type="button" onclick={() => copy(snippet)}>
							{copiedKey === snippet.labelKey ? t('guide.copied') : t('guide.copy')}
						</button>
						{#if onInsert}
							<button type="button" class="primary" onclick={() => onInsert?.(snippet.code)}>
								{t('guide.insert')}
							</button>
						{/if}
					</div>
				</figure>
			{/each}
		</section>
	{/each}
</div>

<style>
	.guide {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	h2 {
		font-size: 1.1rem;
		margin: 0 0 0.3rem;
	}

	.intro {
		margin: 0 0 0.75rem;
		font-size: 0.9rem;
		opacity: 0.75;
	}

	figure {
		margin: 0 0 0.85rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		border-radius: 0.6rem;
		overflow: hidden;
		background: var(--dk-surface);
	}

	figcaption {
		padding: 0.5rem 0.85rem;
		font-size: 0.82rem;
		font-weight: 600;
		border-bottom: 1px solid color-mix(in srgb, var(--dk-ink) 10%, transparent);
	}

	pre {
		margin: 0;
		padding: 0.75rem 0.85rem;
		font-family: ui-monospace, 'SFMono-Regular', Menlo, monospace;
		font-size: 0.82rem;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
		background: color-mix(in srgb, var(--dk-ink) 4%, var(--dk-surface));
	}

	.actions {
		display: flex;
		gap: 0.5rem;
		padding: 0.5rem 0.85rem;
	}

	button {
		font: inherit;
		font-size: 0.8rem;
		font-weight: 600;
		padding: 0.3rem 0.7rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.45rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
	}

	button.primary {
		background: var(--dk-ink);
		color: var(--dk-bg);
	}
</style>
