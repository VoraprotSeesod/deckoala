<script lang="ts">
	import { onMount } from 'svelte';
	import DeckEditor from '$lib/components/DeckEditor.svelte';
	import SharePresent from '$lib/components/SharePresent.svelte';
	import { renderDeck } from '$lib/marp';
	import { sharedAdapter, api, ApiError } from '$lib/api';

	let { data } = $props();
	// The share page loads once for one token — capturing the initial value is
	// intentional (no in-place deck switching, unlike the owner editor route).
	// svelte-ignore state_referenced_locally
	const token = data.token;
	// svelte-ignore state_referenced_locally
	const deck = data.deck; // SharedDeck (+ permission)
	const adapter = sharedAdapter(token);

	let presenting = $state(false);
	let presentMarkdown = $state(deck.markdown);
	let pdfBusy = $state(false);
	let errorMsg = $state('');

	// --- view-mode read-only render ---
	let previewHost = $state<HTMLDivElement | null>(null);

	const PREVIEW_CHROME_CSS = `
		:host { display: block; }
		.marpit svg[data-marpit-svg] {
			display: block;
			width: 100%;
			height: auto;
			border-radius: 6px;
			box-shadow: 0 1px 5px rgba(11, 18, 21, 0.15);
			margin: 0 0 1rem;
		}
	`;

	onMount(() => {
		if (deck.permission !== 'view' || !previewHost) return;
		const shadow = previewHost.attachShadow({ mode: 'open' });
		const marpSheet = new CSSStyleSheet();
		const chrome = new CSSStyleSheet();
		chrome.replaceSync(PREVIEW_CHROME_CSS);
		shadow.adoptedStyleSheets = [marpSheet, chrome];
		try {
			const { html, css } = renderDeck(deck.markdown);
			marpSheet.replaceSync(css);
			shadow.innerHTML = html; // safe: marp html:false
		} catch {
			shadow.innerHTML = `<p style="opacity:.7">Preview failed to render.</p>`;
		}
	});

	async function exportPdf() {
		pdfBusy = true;
		errorMsg = '';
		try {
			await api.shared.downloadPdf(token, deck.title);
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : 'PDF export failed.';
		} finally {
			pdfBusy = false;
		}
	}

	function present(markdown: string) {
		presentMarkdown = markdown;
		presenting = true;
	}
</script>

<svelte:head>
	<title>{deck.title} — Deckoala</title>
</svelte:head>

{#if deck.permission === 'edit'}
	<DeckEditor
		{deck}
		{adapter}
		backHref="/"
		backLabel="Deckoala"
		banner="You're editing a shared deck — your changes are saved for everyone with this link."
		onPresent={present}
	/>
{:else}
	<div class="view">
		<div class="topbar">
			<a class="home" href="/">Deckoala</a>
			<span class="vtitle">{deck.title}</span>
			<span class="badge">Shared view</span>
			<span class="spacer"></span>
			<button class="button" onclick={() => present(deck.markdown)}>Present</button>
			<button class="button" onclick={exportPdf} disabled={pdfBusy}>
				{pdfBusy ? 'Generating…' : 'PDF'}
			</button>
			<a class="button" href={api.shared.exportMdUrl(token)} download>.md</a>
		</div>
		{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}
		<div class="preview-host" bind:this={previewHost}></div>
	</div>
{/if}

{#if presenting}
	<SharePresent markdown={presentMarkdown} onExit={() => (presenting = false)} />
{/if}

<style>
	.view {
		max-width: 60rem;
		margin: 0 auto;
	}

	.topbar {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		flex-wrap: wrap;
		margin-bottom: 1rem;
	}

	.home {
		font-weight: 700;
		text-decoration: none;
		color: var(--dk-ink);
	}

	.vtitle {
		font-size: 1.1rem;
		font-weight: 700;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.badge {
		font-size: 0.72rem;
		font-weight: 700;
		text-transform: uppercase;
		padding: 0.12rem 0.45rem;
		border-radius: 0.3rem;
		background: color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	.spacer {
		flex: 1;
	}

	.button {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.4rem 0.7rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		text-decoration: none;
		cursor: pointer;
	}

	.button:disabled {
		opacity: 0.6;
	}

	.error {
		color: #b3261e;
	}

	.preview-host {
		background: color-mix(in srgb, var(--dk-ink) 4%, var(--dk-bg));
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		border-radius: 0.75rem;
		padding: 1rem;
	}
</style>
