<script lang="ts">
	import DeckEditor from '$lib/components/DeckEditor.svelte';
	import ShareManager from '$lib/components/ShareManager.svelte';
	import { ownerAdapter } from '$lib/api';

	let { data } = $props();

	let shareOpen = $state(false);

	// Recomputed when the route swaps decks; DeckEditor re-seeds off `deck.id`.
	const adapter = $derived(ownerAdapter(data.deck.id));
	const presentHref = $derived(`/present/${data.deck.id}`);
</script>

<DeckEditor
	deck={data.deck}
	{adapter}
	backHref="/app"
	backLabel="← Decks"
	{presentHref}
	extra={shareButton}
/>

{#snippet shareButton()}
	<button class="share-btn" onclick={() => (shareOpen = true)}>Share</button>
{/snippet}

{#if shareOpen}
	<ShareManager deckId={data.deck.id} onClose={() => (shareOpen = false)} />
{/if}

<style>
	.share-btn {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.4rem 0.7rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: var(--dk-ink);
		color: var(--dk-bg);
		cursor: pointer;
	}
</style>
