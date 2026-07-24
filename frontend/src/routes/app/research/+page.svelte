<script lang="ts">
	import { api, ApiError, type ResearchDoc, type ResearchFigure } from '$lib/api';
	import { formatDate, t } from '$lib/i18n.svelte';

	let { data } = $props();

	// svelte-ignore state_referenced_locally
	let docs = $state<ResearchDoc[]>(data.docs);
	let errorMsg = $state('');
	let busy = $state(false);
	let fileInput = $state<HTMLInputElement | null>(null);

	// Expanded document: its preview snippet + extracted figures.
	let openId = $state('');
	let snippet = $state('');
	let figures = $state<ResearchFigure[]>([]);

	function fail(e: unknown) {
		errorMsg = e instanceof ApiError ? e.message : t('common.somethingWrong');
	}

	async function upload(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		busy = true;
		errorMsg = '';
		try {
			await api.research.upload(file);
			docs = await api.research.list();
		} catch (e) {
			fail(e);
		} finally {
			busy = false;
		}
	}

	async function toggle(doc: ResearchDoc) {
		if (openId === doc.id) {
			openId = '';
			return;
		}
		openId = doc.id;
		snippet = '';
		figures = [];
		errorMsg = '';
		try {
			const [preview, figs] = await Promise.all([
				api.research.preview(doc.id),
				api.research.figures(doc.id)
			]);
			snippet = preview.snippet;
			figures = figs;
		} catch (e) {
			fail(e);
		}
	}

	async function remove(doc: ResearchDoc) {
		if (!confirm(t('research.removeConfirm', { name: doc.originalName }))) return;
		errorMsg = '';
		try {
			await api.research.remove(doc.id);
			if (openId === doc.id) openId = '';
			docs = await api.research.list();
		} catch (e) {
			fail(e);
		}
	}
</script>

<svelte:head>
	<title>{t('research.title')} — Deckoala</title>
</svelte:head>

<section>
	<div class="head">
		<h1>{t('research.title')}</h1>
		<a class="button" href="/app">{t('fonts.decksLink')}</a>
	</div>
	<p class="hint">{t('research.hint')}</p>

	{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}

	<button class="upload" onclick={() => fileInput?.click()} disabled={busy}>
		{busy ? t('research.uploading') : t('research.upload')}
	</button>
	<input
		bind:this={fileInput}
		type="file"
		accept="application/pdf,.pdf,.txt,.md,text/plain,text/markdown"
		hidden
		onchange={upload}
	/>

	<h2>{t('research.yourDocs')}</h2>
	{#if docs.length === 0}
		<p class="subtle">{t('research.empty')}</p>
	{:else}
		<ul class="docs">
			{#each docs as doc (doc.id)}
				<li>
					<div class="row">
						<div class="meta">
							<span class="name">{doc.originalName}</span>
							<small>
								{t('research.chars', { n: doc.charCount.toLocaleString() })} ·
								{formatDate(doc.createdAt)}
							</small>
						</div>
						<button onclick={() => toggle(doc)}>
							{openId === doc.id ? t('research.hide') : t('research.view')}
						</button>
						<button class="danger" onclick={() => remove(doc)}>{t('research.remove')}</button>
					</div>

					{#if openId === doc.id}
						<div class="detail">
							<h3>{t('research.preview')}</h3>
							<pre>{snippet || t('research.noPreview')}</pre>
							<h3>{t('research.figures', { n: figures.length })}</h3>
							{#if figures.length === 0}
								<p class="subtle">{t('research.noFigures')}</p>
							{:else}
								<ul class="figures">
									{#each figures as fig (fig.id)}
										<li>
											<img src={fig.url} alt={t('research.figureAlt', { page: fig.page })} loading="lazy" />
											<small>{t('research.page', { n: fig.page })}</small>
										</li>
									{/each}
								</ul>
							{/if}
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	section {
		max-width: 55rem;
		margin: 0 auto;
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
	}

	h1 {
		font-size: 1.6rem;
		margin: 0;
	}

	h2 {
		font-size: 1.05rem;
		margin: 1.25rem 0 0.5rem;
	}

	h3 {
		font-size: 0.85rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		opacity: 0.55;
		margin: 0.75rem 0 0.35rem;
	}

	.hint,
	.subtle {
		opacity: 0.7;
		font-size: 0.9rem;
	}

	.error {
		color: var(--dk-danger);
	}

	.button {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.4rem 0.7rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		color: var(--dk-ink);
		text-decoration: none;
	}

	.upload {
		font: inherit;
		font-weight: 600;
		width: 100%;
		padding: 0.7rem;
		margin-top: 0.5rem;
		border: 1.5px dashed color-mix(in srgb, var(--dk-ink) 40%, transparent);
		border-radius: 0.6rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
	}

	.docs {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	.docs li {
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		border-radius: 0.6rem;
		background: var(--dk-surface);
		padding: 0.6rem 0.9rem;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.meta {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}

	.name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.meta small {
		opacity: 0.65;
		font-size: 0.78rem;
	}

	.row button {
		font: inherit;
		font-size: 0.8rem;
		font-weight: 600;
		padding: 0.3rem 0.7rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 30%, transparent);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
	}

	.row button.danger {
		color: var(--dk-danger);
		border-color: currentColor;
	}

	.detail {
		margin-top: 0.6rem;
		border-top: 1px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	pre {
		margin: 0;
		padding: 0.6rem 0.75rem;
		max-height: 12rem;
		overflow: auto;
		font-size: 0.8rem;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
		border-radius: 0.5rem;
		background: color-mix(in srgb, var(--dk-ink) 5%, var(--dk-surface));
	}

	.figures {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(9rem, 1fr));
		gap: 0.6rem;
	}

	.figures img {
		width: 100%;
		aspect-ratio: 4 / 3;
		object-fit: contain;
		border-radius: 0.4rem;
		background: color-mix(in srgb, var(--dk-ink) 6%, transparent);
	}

	.figures small {
		font-size: 0.72rem;
		opacity: 0.6;
	}
</style>
