<script lang="ts">
	import { tick } from 'svelte';
	import { renderDeck, THEMES } from '$lib/marp';
	import { getScalar, setScalar } from '$lib/frontmatter';
	import { splitDeck } from '$lib/slides';
	import { t } from '$lib/i18n.svelte';

	type Props = {
		open: boolean;
		/** The deck's current markdown (read-only source for the thumbnails). */
		markdown: string;
		/** Apply the chosen theme — parent rewrites frontmatter + autosaves. */
		onApply: (next: string) => void;
		onClose: () => void;
	};

	let { open, markdown, onApply, onClose }: Props = $props();

	let hosts = $state<Array<HTMLDivElement | null>>([]);
	let panel = $state<HTMLDivElement | null>(null);
	let restoreTo: HTMLElement | null = null;

	const current = $derived(getScalar(markdown, 'theme') ?? 'deckoala');

	/** First slide only — a cheap, representative thumbnail. */
	const sampleBody = $derived.by(() => {
		const { slides } = splitDeck(markdown.replace(/\r\n/g, '\n'));
		return slides[0] ?? '# ';
	});

	// Render each theme's thumbnail into its own shadow root. Runs when the
	// modal opens (and if the deck content changes underneath it).
	$effect(() => {
		if (!open) return;
		// track deps
		void sampleBody;
		tick().then(() => {
			for (let i = 0; i < THEMES.length; i++) {
				const host = hosts[i];
				if (!host) continue;
				let root = host.shadowRoot;
				if (!root) root = host.attachShadow({ mode: 'open' });
				try {
					// A throwaway deck: frontmatter forcing this theme + the sample slide.
					const deck = `---\nmarp: true\ntheme: ${THEMES[i].id}\n---\n\n${sampleBody}\n`;
					const { html, css } = renderDeck(deck);
					const sheet = new CSSStyleSheet();
					sheet.replaceSync(css);
					root.adoptedStyleSheets = [sheet];
					const svg = new DOMParser()
						.parseFromString(html, 'text/html')
						.querySelector('svg[data-marpit-svg]');
					root.innerHTML = svg ? svg.outerHTML : '';
				} catch {
					root.innerHTML = '';
				}
			}
		});
	});

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

	function choose(id: string) {
		if (id !== current) onApply(setScalar(markdown, 'theme', id));
		onClose();
	}

	function onKeydown(event: KeyboardEvent) {
		// Only Escape is intercepted. Tab is left alone so keyboard users can move
		// between the theme cards (blanket-trapping it froze focus on the panel).
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
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
			aria-label={t('theme.title')}
			tabindex="-1"
			bind:this={panel}
			onkeydown={onKeydown}
		>
			<header>
				<h2>{t('theme.title')}</h2>
				<button type="button" onclick={onClose}>{t('help.close')}</button>
			</header>
			<p class="hint">{t('theme.hint')}</p>
			<ul class="grid">
				{#each THEMES as theme, i (theme.id)}
					<li>
						<button
							type="button"
							class="card"
							class:active={theme.id === current}
							aria-pressed={theme.id === current}
							onclick={() => choose(theme.id)}
						>
							<div class="thumb" bind:this={hosts[i]}></div>
							<span class="name">
								{t(`theme.name.${theme.id}`)}
								{#if theme.id === current}<span class="badge">{t('theme.current')}</span>{/if}
							</span>
						</button>
					</li>
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
		width: min(48rem, 100%);
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

	.grid {
		list-style: none;
		margin: 0;
		padding: 1rem;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
		gap: 1rem;
		overflow-y: auto;
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		width: 100%;
		padding: 0.5rem;
		font: inherit;
		text-align: left;
		background: transparent;
		border: 2px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.7rem;
		cursor: pointer;
		color: inherit;
	}

	.card:hover {
		border-color: color-mix(in srgb, var(--dk-ink) 40%, transparent);
	}

	.card.active {
		border-color: var(--dk-ink);
	}

	.thumb {
		aspect-ratio: 16 / 9;
		border-radius: 0.4rem;
		overflow: hidden;
		background: #f8f8ff;
	}

	.name {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-weight: 600;
		font-size: 0.9rem;
	}

	.badge {
		font-size: 0.7rem;
		font-weight: 700;
		padding: 0.1rem 0.45rem;
		border-radius: 999px;
		background: var(--dk-ink);
		color: var(--dk-bg);
	}
</style>
