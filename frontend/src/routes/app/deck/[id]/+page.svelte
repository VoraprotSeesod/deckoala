<script lang="ts">
	import { api } from '$lib/api';

	let { data } = $props();
</script>

<svelte:head>
	<title>{data.deck.title} — Deckoala</title>
</svelte:head>

<article>
	<div class="head">
		<div>
			<h1>{data.deck.title}</h1>
			<p class="meta">Updated {new Date(data.deck.updatedAt).toLocaleString()}</p>
		</div>
		<div class="actions">
			<a class="button" href={api.decks.exportUrl(data.deck.id)} download>Export .md</a>
			<a class="button" href="/app">Back to decks</a>
		</div>
	</div>

	<p class="note">The editor with live slide preview arrives in the next build phase — here is the raw Marp Markdown for now.</p>

	<pre>{data.deck.markdown}</pre>
</article>

<style>
	article {
		max-width: 70rem;
		margin: 0 auto;
	}

	.head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
		flex-wrap: wrap;
	}

	h1 {
		font-size: 1.5rem;
		margin: 0;
		overflow-wrap: anywhere;
	}

	.meta {
		font-size: 0.85rem;
		opacity: 0.6;
		margin: 0.25rem 0 0;
	}

	.actions {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.button {
		font-size: 0.9rem;
		font-weight: 600;
		padding: 0.45rem 0.8rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		color: var(--dk-ink);
		text-decoration: none;
	}

	.note {
		opacity: 0.7;
	}

	pre {
		background: #fff;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		padding: 1rem;
		overflow-x: auto;
		font-size: 0.9rem;
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
