<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, ApiError, type DeckMeta } from '$lib/api';

	let { data } = $props();
	let decks = $state<DeckMeta[]>([]);
	$effect(() => {
		decks = data.decks;
	});
	let errorMsg = $state('');
	let busy = $state(false);
	let fileInput = $state<HTMLInputElement | null>(null);

	function fail(e: unknown) {
		errorMsg = e instanceof ApiError ? e.message : 'Something went wrong.';
	}

	function updatedLabel(iso: string): string {
		return new Date(iso).toLocaleString();
	}

	async function newDeck() {
		busy = true;
		errorMsg = '';
		try {
			const deck = await api.decks.create();
			goto(`/app/deck/${deck.id}`);
		} catch (e) {
			fail(e);
		} finally {
			busy = false;
		}
	}

	async function importFile(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		errorMsg = '';
		if (file.size > 1_000_000) {
			errorMsg = 'File too large — decks are capped at 1 MB of Markdown.';
			return;
		}
		try {
			const markdown = await file.text();
			// Code-point-wise cleanup: strip control chars (a Linux filename may
			// legally contain them; the server rejects such titles) and truncate
			// at 200 code points — a plain .slice() could split an emoji
			// surrogate pair and produce JSON that serde_json rejects.
			const stem = file.name.replace(/\.(md|markdown)$/i, '');
			const chars = Array.from(stem).filter((ch) => {
				const code = ch.codePointAt(0) ?? 0;
				return code > 31 && code !== 127;
			});
			const title = chars.slice(0, 200).join('').trim() || undefined;
			const deck = await api.decks.create({ title, markdown });
			decks = [deck, ...decks];
		} catch (e) {
			fail(e);
		}
	}

	async function rename(deck: DeckMeta) {
		const title = prompt('Rename deck', deck.title)?.trim();
		if (!title || title === deck.title) return;
		errorMsg = '';
		try {
			const updated = await api.decks.update(deck.id, { title });
			// The server bumped updated_at, so the deck moves to the top —
			// keep the local order consistent with what a reload would show.
			decks = [updated, ...decks.filter((d) => d.id !== deck.id)];
		} catch (e) {
			fail(e);
		}
	}

	async function duplicate(deck: DeckMeta) {
		errorMsg = '';
		try {
			const copy = await api.decks.duplicate(deck.id);
			decks = [copy, ...decks];
		} catch (e) {
			fail(e);
		}
	}

	async function remove(deck: DeckMeta) {
		if (!confirm(`Delete "${deck.title}"?`)) return;
		errorMsg = '';
		try {
			await api.decks.remove(deck.id);
			decks = decks.filter((d) => d.id !== deck.id);
		} catch (e) {
			fail(e);
		}
	}
</script>

<svelte:head>
	<title>Decks — Deckoala</title>
</svelte:head>

<section>
	<div class="toolbar">
		<h1>Decks</h1>
		<div class="actions">
			<button class="primary" onclick={newDeck} disabled={busy}>New deck</button>
			<button onclick={() => fileInput?.click()}>Import .md</button>
			<input
				bind:this={fileInput}
				type="file"
				accept=".md,.markdown,text/markdown"
				onchange={importFile}
				hidden
			/>
		</div>
	</div>

	{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}

	{#if decks.length === 0}
		<div class="empty">
			<img src="/logo.svg" alt="" width="72" height="72" />
			<p>No decks yet. Your first presentation is one click away.</p>
			<button class="primary" onclick={newDeck} disabled={busy}>New deck</button>
		</div>
	{:else}
		<ul class="grid">
			{#each decks as deck (deck.id)}
				<li class="card">
					<a class="title" href="/app/deck/{deck.id}">{deck.title}</a>
					<span class="meta">Updated {updatedLabel(deck.updatedAt)}</span>
					<div class="row">
						<button onclick={() => rename(deck)}>Rename</button>
						<button onclick={() => duplicate(deck)}>Duplicate</button>
						<a class="button" href={api.decks.exportUrl(deck.id)} download>Export</a>
						<button class="danger" onclick={() => remove(deck)}>Delete</button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	section {
		max-width: 70rem;
		margin: 0 auto;
	}

	.toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		flex-wrap: wrap;
		margin-bottom: 1.25rem;
	}

	h1 {
		font-size: 1.6rem;
		margin: 0;
	}

	.actions {
		display: flex;
		gap: 0.5rem;
	}

	button,
	.button {
		font: inherit;
		font-size: 0.9rem;
		font-weight: 600;
		padding: 0.45rem 0.8rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
		text-decoration: none;
		display: inline-block;
	}

	.primary {
		background: var(--dk-ink);
		color: var(--dk-bg);
	}

	.danger {
		border-color: #b3261e;
		color: #b3261e;
	}

	button:disabled {
		opacity: 0.6;
	}

	.error {
		color: #b3261e;
	}

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 3rem 1rem;
		text-align: center;
		opacity: 0.85;
	}

	.grid {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 17.5rem), 1fr));
		gap: 1rem;
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 1rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		background: #fff;
	}

	.title {
		font-weight: 700;
		font-size: 1.05rem;
		color: inherit;
		text-decoration: none;
		overflow-wrap: anywhere;
	}

	.title:hover {
		text-decoration: underline;
	}

	.meta {
		font-size: 0.85rem;
		opacity: 0.6;
	}

	.row {
		display: flex;
		gap: 0.4rem;
		flex-wrap: wrap;
		margin-top: auto;
	}

	.row button,
	.row .button {
		font-size: 0.8rem;
		padding: 0.3rem 0.6rem;
	}
</style>
