<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, ApiError, type DeckMeta } from '$lib/api';
	import ShareManager from '$lib/components/ShareManager.svelte';
	import { t, formatDate } from '$lib/i18n.svelte';

	let { data } = $props();
	let sharingDeckId = $state<string | null>(null);
	let decks = $state<DeckMeta[]>([]);
	$effect(() => {
		decks = data.decks;
	});
	let errorMsg = $state('');
	let busy = $state(false);
	let fileInput = $state<HTMLInputElement | null>(null);

	function fail(e: unknown) {
		errorMsg = e instanceof ApiError ? e.message : t('common.somethingWrong');
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
			errorMsg = t('dash.fileTooLarge');
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
		const title = prompt(t('dash.renamePrompt'), deck.title)?.trim();
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

	let pdfBusyId = $state<string | null>(null);
	async function exportPdf(deck: DeckMeta) {
		pdfBusyId = deck.id;
		errorMsg = '';
		try {
			await api.decks.downloadPdf(deck.id, deck.title);
		} catch (e) {
			fail(e);
		} finally {
			pdfBusyId = null;
		}
	}

	async function remove(deck: DeckMeta) {
		if (!confirm(t('dash.confirmDelete', { title: deck.title }))) return;
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
	<title>{t('dash.title')} — Deckoala</title>
</svelte:head>

<section>
	<div class="toolbar">
		<h1>{t('dash.title')}</h1>
		<div class="actions">
			<button class="primary" onclick={newDeck} disabled={busy}>{t('dash.newDeck')}</button>
			<button onclick={() => fileInput?.click()}>{t('dash.import')}</button>
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
			<p>{t('dash.emptyTitle')}</p>
			<button class="primary" onclick={newDeck} disabled={busy}>{t('dash.newDeck')}</button>
		</div>
	{:else}
		<ul class="grid">
			{#each decks as deck (deck.id)}
				<li class="card">
					<a class="title" href="/app/deck/{deck.id}">{deck.title}</a>
					<span class="meta">{t('dash.updated', { when: formatDate(deck.updatedAt) })}</span>
					<div class="row">
						<a class="button" href="/present/{deck.id}">{t('dash.present')}</a>
						<button onclick={() => (sharingDeckId = deck.id)}>{t('dash.share')}</button>
						<button onclick={() => exportPdf(deck)} disabled={pdfBusyId === deck.id}>
							{pdfBusyId === deck.id ? t('dash.pdfBusy') : t('dash.pdf')}
						</button>
						<button onclick={() => rename(deck)}>{t('dash.rename')}</button>
						<button onclick={() => duplicate(deck)}>{t('dash.duplicate')}</button>
						<a class="button" href={api.decks.exportUrl(deck.id)} download>{t('dash.exportMd')}</a>
						<button class="danger" onclick={() => remove(deck)}>{t('dash.delete')}</button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</section>

{#if sharingDeckId}
	<ShareManager deckId={sharingDeckId} onClose={() => (sharingDeckId = null)} />
{/if}

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
		border-color: var(--dk-danger);
		color: var(--dk-danger);
	}

	button:disabled {
		opacity: 0.6;
	}

	.error {
		color: var(--dk-danger);
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
		background: var(--dk-surface);
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
